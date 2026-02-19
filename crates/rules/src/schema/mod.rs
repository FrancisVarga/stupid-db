//! YAML DSL schema types with serde deserialization.
//!
//! Defines the complete type hierarchy for rule documents:
//! - `RuleEnvelope`: lightweight first-pass header (apiVersion, kind, metadata)
//! - `RuleDocument`: enum dispatching to kind-specific types
//! - `AnomalyRule`: anomaly detection rules with templates and signal composition
//!
//! New rule kinds (EntitySchema, FeatureConfig, etc.) are added as `RuleDocument` variants.

mod anomaly;
mod composition;
mod document;
mod enrichment;
mod envelope;
mod filters;
mod kind;
mod metadata;
mod notifications;

pub use anomaly::*;
pub use composition::*;
pub use document::*;
pub use enrichment::*;
pub use envelope::*;
pub use filters::*;
pub use kind::*;
pub use metadata::*;
pub use notifications::*;

#[cfg(test)]
mod tests;
