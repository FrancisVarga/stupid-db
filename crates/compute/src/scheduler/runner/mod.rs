//! Compute scheduler runner -- manages worker pools and task execution.
//!
//! Split into focused submodules:
//! - `core`: Scheduler struct, constructor, registration, and accessor methods
//! - `execution`: P0 immediate execution and main scheduling loop
//! - `scheduling`: Task collection with backpressure and dependency filtering

mod core;
mod execution;
mod scheduling;
#[cfg(test)]
mod tests;

pub use self::core::Scheduler;
