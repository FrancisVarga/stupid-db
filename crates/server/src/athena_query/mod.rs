//! AWS Athena query execution utilities.
//!
//! Provides a reusable Athena client builder from decrypted credentials
//! and helper functions for executing queries with polling.

mod execution;
mod schema;
mod types;

pub use execution::*;
pub use schema::*;
