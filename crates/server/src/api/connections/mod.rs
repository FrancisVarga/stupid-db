//! Connection CRUD endpoints for DB, Queue, and Athena connections.
//!
//! SRP: connection credential management (18 handlers total).

mod athena;
mod db;
mod queue;

pub use athena::*;
pub use db::*;
pub use queue::*;
