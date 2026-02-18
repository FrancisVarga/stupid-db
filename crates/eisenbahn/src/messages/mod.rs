//! Domain-specific message types for the eisenbahn messaging layer.
//!
//! This module provides:
//! - **Event messages** (`events`): Notifications published via PUB/SUB
//! - **Pipeline messages** (`pipeline`): Work items distributed via PUSH/PULL
//! - **Topic constants** (`topics`): Canonical topic strings for routing

pub mod events;
pub mod pipeline;
pub mod topics;

pub use events::*;
pub use pipeline::*;
