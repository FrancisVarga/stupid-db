//! Computed analytics endpoints: PageRank, communities, degrees,
//! patterns, co-occurrence, trends, and anomalies.
//!
//! SRP: exposing precomputed knowledge-store results via REST.

mod anomaly;
mod cooccurrence;
mod graph_metrics;
mod patterns;
mod trends;

pub use anomaly::*;
pub use cooccurrence::*;
pub use graph_metrics::*;
pub use patterns::*;
pub use trends::*;

// ── Shared query params ──────────────────────────────────────────

#[derive(serde::Deserialize, utoipa::IntoParams)]
pub struct ComputeQueryParams {
    /// Maximum number of results to return (default 50, max 500).
    pub limit: Option<usize>,
}
