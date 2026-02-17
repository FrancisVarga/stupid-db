use std::sync::Arc;

use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{info, warn};
use uuid::Uuid;

use crate::state::AppState;
use crate::vector_store::{self, ChunkInsert};

// ── Request/Response types ────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    10
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct UploadResponse {
    #[schema(value_type = String)]
    pub document_id: Uuid,
    pub filename: String,
    pub chunk_count: usize,
    pub file_size: i64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SearchResponse {
    #[schema(value_type = Vec<Object>)]
    pub results: Vec<vector_store::SearchResult>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct DocumentListResponse {
    #[schema(value_type = Vec<Object>)]
    pub documents: Vec<vector_store::DocumentRecord>,
}

// ── Helper: check pool + embedder ─────────────────

fn check_embedding_deps(
    state: &AppState,
) -> Result<
    (
        &sqlx::PgPool,
        &Arc<dyn stupid_ingest::embedding::Embedder>,
    ),
    (StatusCode, String),
> {
    let pool = state
        .pg_pool
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "PostgreSQL not configured".to_string()))?;
    let embedder = state
        .embedder
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "Embedding provider not configured".to_string()))?;
    Ok((pool, embedder))
}

// ── POST /embeddings/upload ───────────────────────

/// Upload a document for embedding
///
/// Accepts multipart/form-data with a file field. The document is parsed,
/// chunked, embedded, and stored in pgvector for semantic search.
#[utoipa::path(
    post,
    path = "/embeddings/upload",
    tag = "Embeddings",
    request_body(content_type = "multipart/form-data", description = "File upload"),
    responses(
        (status = 200, description = "Document uploaded and chunked", body = UploadResponse),
        (status = 400, description = "Upload error", body = String)
    )
)]
pub async fn upload(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, (StatusCode, String)> {
    let (pool, embedder) = check_embedding_deps(&state)?;

    // Extract file from multipart
    let field = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Multipart error: {e}")))?
        .ok_or((StatusCode::BAD_REQUEST, "No file provided".to_string()))?;

    let filename = field.file_name().unwrap_or("unnamed").to_string();
    let bytes = field
        .bytes()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Failed to read file: {e}")))?;

    let file_size = bytes.len() as i64;

    // Check file size limit (1GB)
    if file_size > 1024 * 1024 * 1024 {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, "File exceeds 1GB limit".to_string()));
    }

    // Extract text
    let doc = stupid_ingest::document::extract_text(&bytes, &filename)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Text extraction failed: {e}")))?;

    let total_chars = doc.total_chars();
    info!(
        "Extracted '{}' (type={}): {} pages, {} chars",
        filename,
        doc.file_type,
        doc.pages.len(),
        total_chars,
    );

    if total_chars == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Document '{}' ({}) contains no extractable text. \
                 For PDFs, ensure the file contains a text layer (scanned/image PDFs are not supported).",
                filename, doc.file_type
            ),
        ));
    }

    // Chunk the document
    let config = stupid_ingest::document::chunker::ChunkConfig::default();
    let chunks = stupid_ingest::document::chunker::chunk_document(&doc, &config);

    if chunks.is_empty() {
        warn!(
            "Document '{}' has {} chars but produced 0 chunks (min_chunk_tokens={})",
            filename, total_chars, config.min_chunk_tokens
        );
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Document '{}' produced no chunks ({} chars extracted, but below minimum chunk threshold of ~{} words)",
                filename, total_chars, config.min_chunk_tokens
            ),
        ));
    }

    // Embed chunks in batches to avoid API timeouts on large documents
    const EMBED_BATCH_SIZE: usize = 64;
    let texts: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();
    let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(texts.len());
    for (i, batch) in texts.chunks(EMBED_BATCH_SIZE).enumerate() {
        info!(
            "Embedding batch {}/{} ({} chunks)",
            i + 1,
            (texts.len() + EMBED_BATCH_SIZE - 1) / EMBED_BATCH_SIZE,
            batch.len()
        );
        let batch_embeddings = embedder
            .embed_batch(batch)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Embedding failed (batch {}): {e}", i + 1)))?;
        embeddings.extend(batch_embeddings);
    }

    // Insert document
    let document_id = vector_store::insert_document(pool, &filename, &doc.file_type, file_size)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB insert failed: {e}")))?;

    // Persist original file to data/embeddings/{document_id}/
    let embeddings_dir = state.data_dir.join("embeddings").join(document_id.to_string());
    fs::create_dir_all(&embeddings_dir)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create storage dir: {e}")))?;
    fs::write(embeddings_dir.join(&filename), &bytes)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save file: {e}")))?;
    info!("Saved original file to {}", embeddings_dir.join(&filename).display());

    // Insert chunks with embeddings
    let chunk_inserts: Vec<ChunkInsert> = chunks
        .iter()
        .zip(embeddings)
        .map(|(chunk, emb)| ChunkInsert {
            chunk_index: chunk.index,
            content: chunk.content.clone(),
            page_number: chunk.page_number,
            section_heading: chunk.section_heading.clone(),
            embedding: emb,
        })
        .collect();

    let chunk_count = chunk_inserts.len();
    vector_store::insert_chunks(pool, document_id, chunk_inserts)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Chunk insert failed: {e}")))?;

    info!("Uploaded document '{}': {} chunks embedded", filename, chunk_count);

    Ok(Json(UploadResponse {
        document_id,
        filename,
        chunk_count,
        file_size,
    }))
}

// ── POST /embeddings/search ───────────────────────

/// Semantic search across embedded documents
///
/// Embeds the query text and performs a cosine-similarity search against
/// all stored document chunks via pgvector.
#[utoipa::path(
    post,
    path = "/embeddings/search",
    tag = "Embeddings",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Search results ranked by similarity", body = SearchResponse),
        (status = 503, description = "Embedding service unavailable", body = String)
    )
)]
pub async fn search(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, (StatusCode, String)> {
    let (pool, embedder) = check_embedding_deps(&state)?;

    // Embed the query
    let embeddings = embedder
        .embed_batch(&[&req.query])
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Embedding failed: {e}")))?;

    let query_embedding = embeddings
        .into_iter()
        .next()
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No embedding returned".to_string()))?;

    // Search pgvector
    let results = vector_store::search(pool, query_embedding, req.limit)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Search failed: {e}")))?;

    Ok(Json(SearchResponse { results }))
}

// ── GET /embeddings/documents ─────────────────────

/// List all embedded documents
///
/// Returns metadata for every document that has been uploaded and embedded.
#[utoipa::path(
    get,
    path = "/embeddings/documents",
    tag = "Embeddings",
    responses(
        (status = 200, description = "List of embedded documents", body = DocumentListResponse),
        (status = 503, description = "PostgreSQL not configured", body = String)
    )
)]
pub async fn list_documents(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DocumentListResponse>, (StatusCode, String)> {
    let pool = state
        .pg_pool
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "PostgreSQL not configured".to_string()))?;

    let documents = vector_store::list_documents(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to list documents: {e}")))?;

    Ok(Json(DocumentListResponse { documents }))
}

// ── DELETE /embeddings/documents/:id ──────────────

/// Delete an embedded document and its chunks
///
/// Removes the document record, all associated chunks and embeddings from
/// pgvector, and deletes the stored original file from disk.
#[utoipa::path(
    delete,
    path = "/embeddings/documents/{id}",
    tag = "Embeddings",
    params(("id" = String, Path, description = "Document UUID")),
    responses(
        (status = 204, description = "Document deleted"),
        (status = 404, description = "Document not found", body = String)
    )
)]
pub async fn delete_document(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let pool = state
        .pg_pool
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "PostgreSQL not configured".to_string()))?;

    let deleted = vector_store::delete_document(pool, id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Delete failed: {e}")))?;

    if deleted {
        // Clean up stored file
        let embeddings_dir = state.data_dir.join("embeddings").join(id.to_string());
        if let Err(e) = fs::remove_dir_all(&embeddings_dir).await {
            warn!("Failed to remove file dir {}: {e}", embeddings_dir.display());
        }
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, "Document not found".to_string()))
    }
}
