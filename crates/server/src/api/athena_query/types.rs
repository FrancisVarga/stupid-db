//! Request types for Athena query endpoints.

use serde::Deserialize;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AthenaQueryRequest {
    pub sql: String,
    #[serde(default)]
    pub database: Option<String>,
}
