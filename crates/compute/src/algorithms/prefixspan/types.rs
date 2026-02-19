use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Compressed event type representation for sequence mining.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventTypeCompressed(pub String);

impl std::fmt::Display for EventTypeCompressed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Category of a discovered temporal pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternCategory {
    /// Leads to inactivity.
    Churn,
    /// Leads to increased activity.
    Engagement,
    /// Cascading errors.
    ErrorChain,
    /// Conversion sequence.
    Funnel,
    /// Unclassified.
    Unknown,
}

/// A temporal pattern discovered by PrefixSpan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalPattern {
    /// Unique pattern identifier.
    pub id: String,
    /// The event sequence comprising this pattern.
    pub sequence: Vec<EventTypeCompressed>,
    /// Fraction of members exhibiting this pattern.
    pub support: f64,
    /// Absolute number of members exhibiting this pattern.
    pub member_count: usize,
    /// Average time span of the sequence across members.
    pub avg_duration_secs: f64,
    /// When this pattern was first detected.
    pub first_seen: DateTime<Utc>,
    /// Classification of the pattern.
    pub category: PatternCategory,
    /// Optional human-readable description (e.g. LLM-generated).
    pub description: Option<String>,
}

/// Configuration for PrefixSpan mining.
#[derive(Debug, Clone)]
pub struct PrefixSpanConfig {
    /// Minimum fraction of members exhibiting the pattern.
    pub min_support: f64,
    /// Maximum sequence length to mine.
    pub max_length: usize,
    /// Minimum absolute member count for a pattern.
    pub min_members: usize,
}

impl Default for PrefixSpanConfig {
    fn default() -> Self {
        Self {
            min_support: 0.01,
            max_length: 10,
            min_members: 50,
        }
    }
}
