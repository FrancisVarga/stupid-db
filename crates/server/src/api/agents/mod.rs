//! Agent orchestration endpoints: single agent execution, team execution,
//! and session-based chat with history.
//!
//! SRP: agent/team lifecycle and session management.

mod crud;
mod execute;
mod sessions;
mod sessions_execute;
mod sessions_stream;
mod types;

// ── Re-exports ───────────────────────────────────────────────────
// Preserves flat `agents::foo` import paths used by api/mod.rs.

pub use crud::*;
pub use execute::*;
pub use sessions::*;
pub use sessions_execute::*;
pub use sessions_stream::*;
pub use types::{
    AgentExecuteRequest,
    TeamExecuteRequest,
    SessionCreateRequest, SessionUpdateRequest,
    SessionExecuteAgentRequest, SessionExecuteTeamRequest,
    SessionExecuteRequest,
    SessionStreamRequest,
};
