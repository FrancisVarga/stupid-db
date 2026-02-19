//! Stille Post CRUD endpoints for agents, pipelines, data sources,
//! schedules, runs, reports, and deliveries.
//!
//! All endpoints require a PostgreSQL pool (`pg_pool`) on AppState.

mod agents;
mod common;
mod data_sources;
mod deliveries;
mod pipelines;
mod runs;
mod schedules;
mod yaml_io;
pub(crate) mod yaml_types;

// ── Re-exports ───────────────────────────────────────────────────
// Preserves flat `stille_post::sp_*` import paths used by api/mod.rs.

pub use agents::*;
pub use data_sources::*;
pub use deliveries::*;
pub use pipelines::*;
pub use runs::*;
pub use schedules::*;
pub use yaml_io::*;
