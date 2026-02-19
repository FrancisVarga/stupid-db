use std::path::Path;

use serde_json::Value;
use stupid_catalog::{Catalog, QueryExecutor, QueryPlan};
use stupid_graph::GraphStore;
use tracing::{debug, info};

use crate::provider::{LlmError, LlmProvider, Message, Role};

/// Path to the externalized query planner system prompt template.
const QUERY_PLANNER_TEMPLATE_PATH: &str = "data/bundeswehr/prompts/query-planner-system.md";

/// Placeholder in the template that gets replaced with the catalog schema.
const SCHEMA_PLACEHOLDER: &str = "<<<schema>>>";

/// Converts natural language questions into QueryPlans via an LLM,
/// then executes them against the GraphStore.
pub struct QueryGenerator {
    provider: Box<dyn LlmProvider>,
    temperature: f32,
    max_tokens: u32,
    /// The system prompt template loaded from disk at construction time.
    system_prompt_template: String,
}

impl QueryGenerator {
    pub fn new(provider: Box<dyn LlmProvider>, temperature: f32, max_tokens: u32) -> Self {
        let system_prompt_template = load_template(QUERY_PLANNER_TEMPLATE_PATH)
            .expect("query planner system prompt template must exist at startup");
        Self {
            provider,
            temperature,
            max_tokens,
            system_prompt_template,
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
        let system_prompt = self
            .system_prompt_template
            .replace(SCHEMA_PLACEHOLDER, &catalog.to_system_prompt());
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

/// Load a prompt template from disk, failing eagerly with a clear message.
fn load_template(path: &str) -> Result<String, String> {
    let path = Path::new(path);
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read prompt template at {}: {e}", path.display()))?;

    let count = content.matches(SCHEMA_PLACEHOLDER).count();
    if count != 1 {
        return Err(format!(
            "prompt template at {} must contain exactly one '{SCHEMA_PLACEHOLDER}' placeholder, found {count}",
            path.display()
        ));
    }

    Ok(content)
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

    /// Resolve the template path relative to the workspace root (two levels up from CARGO_MANIFEST_DIR).
    fn workspace_template_path() -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_root = std::path::Path::new(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        workspace_root
            .join(QUERY_PLANNER_TEMPLATE_PATH)
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn template_file_exists_and_has_placeholder() {
        let path = workspace_template_path();
        let template = load_template(&path)
            .expect("template file must exist at data/bundeswehr/prompts/query-planner-system.md");
        assert!(
            template.contains(SCHEMA_PLACEHOLDER),
            "template must contain the <<<schema>>> placeholder"
        );
        assert_eq!(
            template.matches(SCHEMA_PLACEHOLDER).count(),
            1,
            "template must contain exactly one <<<schema>>> placeholder"
        );
        assert!(
            template.contains("QueryPlan"),
            "template must describe QueryPlan format"
        );
    }

    #[test]
    fn template_schema_replacement_works() {
        let path = workspace_template_path();
        let template = load_template(&path).unwrap();
        let catalog = Catalog {
            entity_types: vec![],
            edge_types: vec![],
            total_nodes: 100,
            total_edges: 200,
            external_sources: vec![],
        };
        let prompt = template.replace(SCHEMA_PLACEHOLDER, &catalog.to_system_prompt());
        assert!(prompt.contains("100 nodes"));
        assert!(prompt.contains("QueryPlan"));
        assert!(!prompt.contains(SCHEMA_PLACEHOLDER));
    }
}
