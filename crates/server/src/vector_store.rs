use sqlx::{PgPool, Row};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::Serialize;
use pgvector::Vector;

// ── Types ──────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct DocumentRecord {
    pub id: Uuid,
    pub filename: String,
    pub file_type: String,
    pub file_size: i64,
    pub uploaded_at: DateTime<Utc>,
    pub chunk_count: i64,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub chunk_id: Uuid,
    pub document_id: Uuid,
    pub filename: String,
    pub content: String,
    pub chunk_index: i32,
    pub page_number: Option<i32>,
    pub section_heading: Option<String>,
    pub similarity: f64,
}

pub struct ChunkInsert {
    pub chunk_index: usize,
    pub content: String,
    pub page_number: Option<usize>,
    pub section_heading: Option<String>,
    pub embedding: Vec<f32>,
}

// ── Operations ─────────────────────────────────────

/// Insert a new document record.
pub async fn insert_document(
    pool: &PgPool,
    filename: &str,
    file_type: &str,
    file_size: i64,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO documents (id, filename, file_type, file_size) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(filename)
    .bind(file_type)
    .bind(file_size)
    .execute(pool)
    .await?;
    Ok(id)
}

/// Insert chunks with embeddings for a document.
pub async fn insert_chunks(
    pool: &PgPool,
    document_id: Uuid,
    chunks: Vec<ChunkInsert>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut ids = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let id = Uuid::new_v4();
        let embedding = Vector::from(chunk.embedding);
        sqlx::query(
            "INSERT INTO chunks (id, document_id, chunk_index, content, page_number, section_heading, embedding) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(id)
        .bind(document_id)
        .bind(chunk.chunk_index as i32)
        .bind(&chunk.content)
        .bind(chunk.page_number.map(|p| p as i32))
        .bind(&chunk.section_heading)
        .bind(&embedding)
        .execute(pool)
        .await?;
        ids.push(id);
    }
    Ok(ids)
}

/// Search chunks by cosine similarity.
pub async fn search(
    pool: &PgPool,
    query_embedding: Vec<f32>,
    limit: i64,
) -> Result<Vec<SearchResult>, sqlx::Error> {
    let embedding = Vector::from(query_embedding);
    let rows = sqlx::query(
        "SELECT c.id, c.document_id, d.filename, c.content, c.chunk_index, \
         c.page_number, c.section_heading, \
         1.0 - (c.embedding <=> $1::vector) as similarity \
         FROM chunks c \
         JOIN documents d ON d.id = c.document_id \
         ORDER BY c.embedding <=> $1::vector \
         LIMIT $2",
    )
    .bind(&embedding)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let results = rows
        .iter()
        .map(|row| SearchResult {
            chunk_id: row.get("id"),
            document_id: row.get("document_id"),
            filename: row.get("filename"),
            content: row.get("content"),
            chunk_index: row.get("chunk_index"),
            page_number: row.get("page_number"),
            section_heading: row.get("section_heading"),
            similarity: row.get("similarity"),
        })
        .collect();
    Ok(results)
}

/// List all documents with chunk counts.
pub async fn list_documents(pool: &PgPool) -> Result<Vec<DocumentRecord>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT d.id, d.filename, d.file_type, d.file_size, d.uploaded_at, \
         COUNT(c.id) as chunk_count \
         FROM documents d \
         LEFT JOIN chunks c ON c.document_id = d.id \
         GROUP BY d.id \
         ORDER BY d.uploaded_at DESC",
    )
    .fetch_all(pool)
    .await?;

    let docs = rows
        .iter()
        .map(|row| DocumentRecord {
            id: row.get("id"),
            filename: row.get("filename"),
            file_type: row.get("file_type"),
            file_size: row.get("file_size"),
            uploaded_at: row.get("uploaded_at"),
            chunk_count: row.get("chunk_count"),
        })
        .collect();
    Ok(docs)
}

/// Delete a document and all its chunks (CASCADE).
pub async fn delete_document(pool: &PgPool, document_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM documents WHERE id = $1")
        .bind(document_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_insert_construction() {
        let chunk = ChunkInsert {
            chunk_index: 0,
            content: "Hello world".to_string(),
            page_number: Some(1),
            section_heading: Some("Intro".to_string()),
            embedding: vec![0.1, 0.2, 0.3],
        };
        assert_eq!(chunk.chunk_index, 0);
        assert_eq!(chunk.content, "Hello world");
        assert_eq!(chunk.page_number, Some(1));
        assert_eq!(chunk.embedding.len(), 3);
    }

    #[test]
    fn document_record_serializes() {
        let rec = DocumentRecord {
            id: Uuid::nil(),
            filename: "test.pdf".to_string(),
            file_type: "pdf".to_string(),
            file_size: 1024,
            uploaded_at: chrono::Utc::now(),
            chunk_count: 5,
        };
        let json = serde_json::to_string(&rec).unwrap();
        assert!(json.contains("\"filename\":\"test.pdf\""));
        assert!(json.contains("\"chunk_count\":5"));
        assert!(json.contains("\"file_size\":1024"));
    }

    #[test]
    fn search_result_serializes() {
        let res = SearchResult {
            chunk_id: Uuid::nil(),
            document_id: Uuid::nil(),
            filename: "doc.txt".to_string(),
            content: "some text".to_string(),
            chunk_index: 2,
            page_number: Some(1),
            section_heading: None,
            similarity: 0.95,
        };
        let json = serde_json::to_string(&res).unwrap();
        assert!(json.contains("\"similarity\":0.95"));
        assert!(json.contains("\"chunk_index\":2"));
        assert!(json.contains("\"section_heading\":null"));
    }
}
