use serde::{Deserialize, Serialize};

/// A structured query plan represented as a DAG of steps.
///
/// The LLM generates this JSON structure, and the QueryExecutor runs it
/// against the GraphStore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPlan {
    pub steps: Vec<QueryStep>,
}

/// A single step in a query plan. Each step has a unique ID and can
/// depend on outputs from previous steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryStep {
    pub id: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(flatten)]
    pub kind: StepKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StepKind {
    #[serde(rename = "filter")]
    Filter(FilterStep),
    #[serde(rename = "traversal")]
    Traversal(TraversalStep),
    #[serde(rename = "aggregate")]
    Aggregate(AggregateStep),
}

/// Filter nodes by entity type and optional field matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterStep {
    pub entity_type: String,
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub operator: Option<FilterOperator>,
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator {
    Equals,
    Contains,
    StartsWith,
}

/// Traverse edges from a set of nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalStep {
    pub edge_type: String,
    #[serde(default = "default_direction")]
    pub direction: TraversalDirection,
    #[serde(default = "default_depth")]
    pub depth: usize,
}

fn default_direction() -> TraversalDirection {
    TraversalDirection::Outgoing
}

fn default_depth() -> usize {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraversalDirection {
    Outgoing,
    Incoming,
    Both,
}

/// Aggregate results by a field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateStep {
    pub group_by: String,
    #[serde(default = "default_metric")]
    pub metric: AggregateMetric,
}

fn default_metric() -> AggregateMetric {
    AggregateMetric::Count
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateMetric {
    Count,
    Sum,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_query_plan() {
        let json = r#"{
            "steps": [
                {
                    "id": "step1",
                    "type": "filter",
                    "entity_type": "Member",
                    "field": "key",
                    "operator": "contains",
                    "value": "alice"
                },
                {
                    "id": "step2",
                    "depends_on": ["step1"],
                    "type": "traversal",
                    "edge_type": "LoggedInFrom",
                    "direction": "outgoing",
                    "depth": 1
                },
                {
                    "id": "step3",
                    "depends_on": ["step2"],
                    "type": "aggregate",
                    "group_by": "entity_type",
                    "metric": "count"
                }
            ]
        }"#;

        let plan: QueryPlan = serde_json::from_str(json).unwrap();
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.steps[0].id, "step1");
        assert_eq!(plan.steps[1].depends_on, vec!["step1"]);
    }
}
