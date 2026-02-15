use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};
use stupid_core::NodeId;
use stupid_graph::GraphStore;
use thiserror::Error;
use tracing::debug;

use crate::plan::*;

#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("unknown step dependency: {0}")]
    UnknownDependency(String),
    #[error("unknown entity type: {0}")]
    UnknownEntityType(String),
    #[error("unknown edge type: {0}")]
    UnknownEdgeType(String),
    #[error("step '{0}' has no input nodes (missing depends_on?)")]
    NoInput(String),
}

/// Hard limit on the number of result rows returned to the client.
const MAX_RESULT_ROWS: usize = 200;

/// Hard limit on intermediate node sets to prevent runaway traversals.
const MAX_INTERMEDIATE_NODES: usize = 50_000;

/// Executes a QueryPlan against a GraphStore, returning results as JSON values.
pub struct QueryExecutor;

impl QueryExecutor {
    /// Execute a query plan and return results as a list of JSON objects.
    pub fn execute(plan: &QueryPlan, graph: &GraphStore) -> Result<Vec<Value>, ExecutorError> {
        // step_id -> set of node IDs produced by that step
        let mut step_results: HashMap<String, HashSet<NodeId>> = HashMap::new();

        for step in &plan.steps {
            // Gather input nodes from dependencies
            let input_nodes: Option<HashSet<NodeId>> = if step.depends_on.is_empty() {
                None // no dependency = operate on full graph
            } else {
                let mut combined = HashSet::new();
                for dep_id in &step.depends_on {
                    let dep = step_results
                        .get(dep_id)
                        .ok_or_else(|| ExecutorError::UnknownDependency(dep_id.clone()))?;
                    combined.extend(dep);
                }
                Some(combined)
            };

            let result = match &step.kind {
                StepKind::Filter(f) => {
                    let mut nodes = Self::exec_filter(f, graph, input_nodes.as_ref());
                    if nodes.len() > MAX_INTERMEDIATE_NODES {
                        debug!(
                            "Step '{}' filter capped from {} to {} nodes",
                            step.id, nodes.len(), MAX_INTERMEDIATE_NODES
                        );
                        nodes = nodes.into_iter().take(MAX_INTERMEDIATE_NODES).collect();
                    }
                    nodes
                }
                StepKind::Traversal(t) => {
                    let input = input_nodes
                        .ok_or_else(|| ExecutorError::NoInput(step.id.clone()))?;
                    let mut nodes = Self::exec_traversal(t, graph, &input);
                    if nodes.len() > MAX_INTERMEDIATE_NODES {
                        debug!(
                            "Step '{}' traversal capped from {} to {} nodes",
                            step.id, nodes.len(), MAX_INTERMEDIATE_NODES
                        );
                        nodes = nodes.into_iter().take(MAX_INTERMEDIATE_NODES).collect();
                    }
                    nodes
                }
                StepKind::Aggregate(a) => {
                    let nodes = input_nodes.unwrap_or_else(|| {
                        graph.nodes.keys().copied().collect()
                    });
                    let agg = Self::exec_aggregate(a, graph, &nodes);
                    // For aggregate, we return early with the aggregation result
                    step_results.insert(step.id.clone(), nodes);
                    debug!("Step '{}' aggregate: {} groups", step.id, agg.len());
                    if step.id == plan.steps.last().map(|s| s.id.as_str()).unwrap_or("") {
                        return Ok(agg);
                    }
                    continue;
                }
            };

            debug!("Step '{}': {} nodes", step.id, result.len());
            step_results.insert(step.id.clone(), result);
        }

        // Return the final step's nodes as JSON
        let last_step_id = plan
            .steps
            .last()
            .map(|s| s.id.as_str())
            .unwrap_or("");
        let final_nodes = step_results
            .get(last_step_id)
            .cloned()
            .unwrap_or_default();

        let total_matched = final_nodes.len();
        let mut results: Vec<Value> = final_nodes
            .iter()
            .take(MAX_RESULT_ROWS)
            .filter_map(|id| {
                let node = graph.nodes.get(id)?;
                Some(json!({
                    "id": id.to_string(),
                    "entity_type": node.entity_type.to_string(),
                    "key": node.key,
                }))
            })
            .collect();

        if total_matched > MAX_RESULT_ROWS {
            results.push(json!({
                "_truncated": true,
                "_total_matched": total_matched,
                "_returned": MAX_RESULT_ROWS,
                "_message": format!(
                    "Result set truncated: {} of {} matches returned. Refine your query or use an aggregate step.",
                    MAX_RESULT_ROWS, total_matched
                ),
            }));
        }

        Ok(results)
    }

    fn exec_filter(
        filter: &FilterStep,
        graph: &GraphStore,
        input: Option<&HashSet<NodeId>>,
    ) -> HashSet<NodeId> {
        let candidates: Box<dyn Iterator<Item = (&NodeId, &stupid_graph::store::Node)>> =
            if let Some(input_set) = input {
                Box::new(
                    input_set
                        .iter()
                        .filter_map(|id| graph.nodes.get(id).map(|n| (id, n))),
                )
            } else {
                Box::new(graph.nodes.iter())
            };

        candidates
            .filter(|(_, node)| {
                // Match entity type (case-insensitive)
                if !node
                    .entity_type
                    .to_string()
                    .eq_ignore_ascii_case(&filter.entity_type)
                {
                    return false;
                }

                // If field/operator/value specified, apply field filter
                if let (Some(field), Some(op), Some(value)) =
                    (&filter.field, &filter.operator, &filter.value)
                {
                    let field_value = match field.as_str() {
                        "key" => &node.key,
                        _ => return false, // unknown field
                    };

                    match op {
                        FilterOperator::Equals => {
                            field_value.eq_ignore_ascii_case(value)
                        }
                        FilterOperator::Contains => {
                            field_value.to_lowercase().contains(&value.to_lowercase())
                        }
                        FilterOperator::StartsWith => {
                            field_value
                                .to_lowercase()
                                .starts_with(&value.to_lowercase())
                        }
                    }
                } else {
                    true // no field filter = match all of this entity type
                }
            })
            .map(|(id, _)| *id)
            .collect()
    }

    fn exec_traversal(
        traversal: &TraversalStep,
        graph: &GraphStore,
        input: &HashSet<NodeId>,
    ) -> HashSet<NodeId> {
        let mut result = HashSet::new();
        let edge_type_str = &traversal.edge_type;

        for &node_id in input {
            // Outgoing edges
            if matches!(
                traversal.direction,
                TraversalDirection::Outgoing | TraversalDirection::Both
            ) {
                if let Some(edge_ids) = graph.outgoing.get(&node_id) {
                    for eid in edge_ids {
                        if let Some(edge) = graph.edges.get(eid) {
                            if edge.edge_type.to_string().eq_ignore_ascii_case(edge_type_str) {
                                result.insert(edge.target);
                            }
                        }
                    }
                }
            }

            // Incoming edges
            if matches!(
                traversal.direction,
                TraversalDirection::Incoming | TraversalDirection::Both
            ) {
                if let Some(edge_ids) = graph.incoming.get(&node_id) {
                    for eid in edge_ids {
                        if let Some(edge) = graph.edges.get(eid) {
                            if edge.edge_type.to_string().eq_ignore_ascii_case(edge_type_str) {
                                result.insert(edge.source);
                            }
                        }
                    }
                }
            }
        }

        result
    }

    fn exec_aggregate(
        aggregate: &AggregateStep,
        graph: &GraphStore,
        nodes: &HashSet<NodeId>,
    ) -> Vec<Value> {
        let mut groups: HashMap<String, usize> = HashMap::new();

        for node_id in nodes {
            if let Some(node) = graph.nodes.get(node_id) {
                let group_key = match aggregate.group_by.as_str() {
                    "entity_type" => node.entity_type.to_string(),
                    "key" => node.key.clone(),
                    _ => "unknown".to_string(),
                };
                *groups.entry(group_key).or_default() += 1;
            }
        }

        let mut result: Vec<Value> = groups
            .into_iter()
            .map(|(group, count)| {
                json!({
                    "group": group,
                    "count": count,
                })
            })
            .collect();

        result.sort_by(|a, b| {
            let ca = a["count"].as_u64().unwrap_or(0);
            let cb = b["count"].as_u64().unwrap_or(0);
            cb.cmp(&ca)
        });

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stupid_core::{EdgeType, EntityType};

    fn build_test_graph() -> GraphStore {
        let mut g = GraphStore::new();
        let seg = "test".to_string();

        let a = g.upsert_node(EntityType::Member, "alice", &seg);
        let b = g.upsert_node(EntityType::Member, "bob", &seg);
        let d1 = g.upsert_node(EntityType::Device, "iphone-1", &seg);
        let d2 = g.upsert_node(EntityType::Device, "android-1", &seg);

        g.add_edge(a, d1, EdgeType::LoggedInFrom, &seg);
        g.add_edge(a, d2, EdgeType::LoggedInFrom, &seg);
        g.add_edge(b, d1, EdgeType::LoggedInFrom, &seg);
        g
    }

    #[test]
    fn execute_filter_only() {
        let g = build_test_graph();
        let plan: QueryPlan = serde_json::from_str(
            r#"{"steps":[{"id":"s1","type":"filter","entity_type":"Member"}]}"#,
        )
        .unwrap();

        let results = QueryExecutor::execute(&plan, &g).unwrap();
        assert_eq!(results.len(), 2); // alice + bob
    }

    #[test]
    fn execute_filter_then_traverse() {
        let g = build_test_graph();
        let plan: QueryPlan = serde_json::from_str(
            r#"{"steps":[
                {"id":"s1","type":"filter","entity_type":"Member","field":"key","operator":"equals","value":"alice"},
                {"id":"s2","depends_on":["s1"],"type":"traversal","edge_type":"LoggedInFrom","direction":"outgoing","depth":1}
            ]}"#,
        )
        .unwrap();

        let results = QueryExecutor::execute(&plan, &g).unwrap();
        // alice -> iphone-1, android-1
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn execute_aggregate() {
        let g = build_test_graph();
        let plan: QueryPlan = serde_json::from_str(
            r#"{"steps":[
                {"id":"s1","type":"aggregate","group_by":"entity_type","metric":"count"}
            ]}"#,
        )
        .unwrap();

        let results = QueryExecutor::execute(&plan, &g).unwrap();
        assert_eq!(results.len(), 2); // Member + Device groups
    }
}
