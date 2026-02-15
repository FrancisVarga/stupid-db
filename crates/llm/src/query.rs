use serde_json::Value;
use stupid_catalog::{Catalog, QueryExecutor, QueryPlan};
use stupid_graph::GraphStore;
use tracing::{debug, info};

use crate::provider::{LlmError, LlmProvider, Message, Role};

/// Converts natural language questions into QueryPlans via an LLM,
/// then executes them against the GraphStore.
pub struct QueryGenerator {
    provider: Box<dyn LlmProvider>,
    temperature: f32,
    max_tokens: u32,
}

impl QueryGenerator {
    pub fn new(provider: Box<dyn LlmProvider>, temperature: f32, max_tokens: u32) -> Self {
        Self {
            provider,
            temperature,
            max_tokens,
        }
    }

    /// Build from config, creating the appropriate provider.
    pub fn from_config(
        llm_config: &stupid_core::config::LlmConfig,
        ollama_config: &stupid_core::config::OllamaConfig,
    ) -> Result<Self, LlmError> {
        let provider = crate::providers::create_provider(llm_config, ollama_config)?;
        Ok(Self::new(provider, llm_config.temperature, llm_config.max_tokens))
    }

    /// Generate a QueryPlan from a natural language question.
    pub async fn generate_plan(
        &self,
        question: &str,
        catalog: &Catalog,
    ) -> Result<QueryPlan, QueryError> {
        let system_prompt = build_system_prompt(catalog);
        let user_prompt = format!(
            "Convert this question to a QueryPlan JSON:\n\n{}\n\nRespond ONLY with valid JSON, no explanation.",
            question
        );

        info!("Generating query plan for: {}", question);

        let messages = vec![
            Message {
                role: Role::System,
                content: system_prompt,
            },
            Message {
                role: Role::User,
                content: user_prompt,
            },
        ];

        let response = self
            .provider
            .complete(messages, self.temperature, self.max_tokens)
            .await
            .map_err(QueryError::LlmError)?;

        debug!("LLM response: {}", response);

        // Extract JSON from response (handle markdown code blocks)
        let json_str = extract_json(&response);

        let plan: QueryPlan =
            serde_json::from_str(json_str).map_err(|e| QueryError::InvalidPlan {
                reason: e.to_string(),
                raw_response: response.clone(),
            })?;

        info!("Generated plan with {} steps", plan.steps.len());
        Ok(plan)
    }

    /// End-to-end: question → plan → execute → results.
    pub async fn ask(
        &self,
        question: &str,
        catalog: &Catalog,
        graph: &GraphStore,
    ) -> Result<QueryResult, QueryError> {
        let plan = self.generate_plan(question, catalog).await?;

        let results = QueryExecutor::execute(&plan, graph).map_err(|e| QueryError::ExecutionError {
            reason: e.to_string(),
        })?;

        Ok(QueryResult {
            question: question.to_string(),
            plan,
            results,
        })
    }
}

/// Full result of a natural language query.
#[derive(Debug, serde::Serialize)]
pub struct QueryResult {
    pub question: String,
    pub plan: QueryPlan,
    pub results: Vec<Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error("LLM error: {0}")]
    LlmError(LlmError),
    #[error("invalid query plan: {reason}")]
    InvalidPlan {
        reason: String,
        raw_response: String,
    },
    #[error("execution error: {reason}")]
    ExecutionError { reason: String },
}

/// Build the system prompt that teaches the LLM about our schema and query format.
fn build_system_prompt(catalog: &Catalog) -> String {
    let schema = catalog.to_system_prompt();

    format!(
        r#"You are a query planner for a graph database. Your job is to convert natural language questions into structured QueryPlan JSON.

## Graph Schema
{schema}

## QueryPlan Format
A QueryPlan is a JSON object with a "steps" array. Each step has:
- "id": unique string identifier
- "depends_on": array of step IDs this step depends on (empty if none)
- "type": one of "filter", "traversal", "aggregate"

### Step Types

**filter** — Select nodes by entity type and optional field matching:
```json
{{"id": "s1", "type": "filter", "entity_type": "Member", "field": "key", "operator": "equals", "value": "alice"}}
```
Operators: "equals", "contains", "starts_with"
If no field/operator/value, matches all nodes of that entity_type.

**traversal** — Follow edges from input nodes:
```json
{{"id": "s2", "depends_on": ["s1"], "type": "traversal", "edge_type": "LoggedInFrom", "direction": "outgoing", "depth": 1}}
```
Directions: "outgoing", "incoming", "both"

**aggregate** — Group and count results:
```json
{{"id": "s3", "depends_on": ["s2"], "type": "aggregate", "group_by": "entity_type", "metric": "count"}}
```
group_by options: "entity_type", "key"

## Rules
- Always start with a "filter" step to select the starting nodes
- Use "traversal" to follow edges between entities
- Use "aggregate" as the final step when the question asks for counts or summaries
- ALWAYS include field/operator/value in filter steps to narrow results — never filter by entity_type alone without a specific value
- If the user asks a broad question (e.g. "show all members"), add an aggregate step to summarize instead of returning raw nodes
- Prefer aggregation over raw node listing — return counts and summaries, not dumps of data
- When a traversal could fan out to thousands of nodes, aggregate the results
- Respond with ONLY valid JSON, no explanation or markdown"#
    )
}

/// Extract JSON from an LLM response, handling markdown code blocks.
fn extract_json(response: &str) -> &str {
    let trimmed = response.trim();

    // Handle ```json ... ``` blocks
    if let Some(start) = trimmed.find("```json") {
        let json_start = start + 7;
        if let Some(end) = trimmed[json_start..].find("```") {
            return trimmed[json_start..json_start + end].trim();
        }
    }

    // Handle ``` ... ``` blocks
    if let Some(start) = trimmed.find("```") {
        let json_start = start + 3;
        // Skip past any language identifier on the same line
        let after_tick = &trimmed[json_start..];
        let content_start = after_tick.find('\n').map_or(0, |n| n + 1);
        if let Some(end) = after_tick[content_start..].find("```") {
            return after_tick[content_start..content_start + end].trim();
        }
    }

    // Try raw JSON (starts with {)
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            return &trimmed[start..=end];
        }
    }

    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_raw() {
        let input = r#"{"steps": []}"#;
        assert_eq!(extract_json(input), r#"{"steps": []}"#);
    }

    #[test]
    fn extract_json_code_block() {
        let input = "Here is the plan:\n```json\n{\"steps\": []}\n```\nDone.";
        assert_eq!(extract_json(input), r#"{"steps": []}"#);
    }

    #[test]
    fn extract_json_with_prefix() {
        let input = "Sure! Here's the query plan: {\"steps\": []}";
        assert_eq!(extract_json(input), r#"{"steps": []}"#);
    }

    #[test]
    fn build_system_prompt_includes_schema() {
        let catalog = Catalog {
            entity_types: vec![],
            edge_types: vec![],
            total_nodes: 100,
            total_edges: 200,
        };
        let prompt = build_system_prompt(&catalog);
        assert!(prompt.contains("100 nodes"));
        assert!(prompt.contains("QueryPlan"));
    }
}
