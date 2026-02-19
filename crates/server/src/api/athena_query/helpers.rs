//! Eisenbahn helper functions for Athena endpoints.

use axum::Json;
use axum::response::sse::Event;

use crate::api::QueryErrorResponse;

/// Map an eisenbahn error to an HTTP error response for Athena endpoints.
pub(crate) fn eb_athena_error(e: stupid_eisenbahn::EisenbahnError) -> (axum::http::StatusCode, Json<QueryErrorResponse>) {
    let status = match &e {
        stupid_eisenbahn::EisenbahnError::Timeout(_) => axum::http::StatusCode::GATEWAY_TIMEOUT,
        _ => axum::http::StatusCode::BAD_GATEWAY,
    };
    (status, Json(QueryErrorResponse { error: e.to_string() }))
}

/// Convert an AthenaServiceResponse to an SSE Event.
pub(crate) fn athena_response_to_sse(resp: stupid_eisenbahn::services::AthenaServiceResponse) -> Event {
    use stupid_eisenbahn::services::AthenaServiceResponse;
    match resp {
        AthenaServiceResponse::Status { state, stats } => {
            Event::default().event("status").data(
                serde_json::json!({ "state": state, "stats": stats }).to_string(),
            )
        }
        AthenaServiceResponse::Columns { columns } => {
            let cols: Vec<serde_json::Value> = columns
                .iter()
                .map(|c| serde_json::json!({ "name": c.name, "type": c.data_type }))
                .collect();
            Event::default()
                .event("columns")
                .data(serde_json::json!({ "columns": cols }).to_string())
        }
        AthenaServiceResponse::Rows { rows } => {
            Event::default()
                .event("rows")
                .data(serde_json::json!({ "rows": rows }).to_string())
        }
        AthenaServiceResponse::Done { total_rows } => {
            Event::default()
                .event("done")
                .data(serde_json::json!({ "total_rows": total_rows }).to_string())
        }
        AthenaServiceResponse::Error { message } => {
            Event::default()
                .event("error")
                .data(serde_json::json!({ "message": message }).to_string())
        }
        AthenaServiceResponse::SchemaRefreshed { status } => {
            Event::default()
                .event("done")
                .data(serde_json::json!({ "status": status }).to_string())
        }
        AthenaServiceResponse::Parquet { .. } => {
            // Parquet data shouldn't appear in SSE stream, but handle gracefully.
            Event::default()
                .event("error")
                .data(r#"{"message":"unexpected parquet data in SSE stream"}"#)
        }
    }
}
