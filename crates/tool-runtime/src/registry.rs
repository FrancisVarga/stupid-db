use crate::tool::{Tool, ToolDefinition};
use std::collections::HashMap;
use std::sync::Arc;

/// Manages available tools, their schemas, and lookup.
/// Thread-safe via Arc wrapping of individual tools.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Returns error if name already registered.
    pub fn register(&mut self, tool: impl Tool + 'static) -> Result<(), RegistryError> {
        let def = tool.definition();
        if self.tools.contains_key(&def.name) {
            return Err(RegistryError::DuplicateName(def.name));
        }
        self.tools.insert(def.name, Arc::new(tool));
        Ok(())
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// List all registered tool definitions (for sending to LLM).
    pub fn list(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("Tool with name '{0}' is already registered")]
    DuplicateName(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::EchoTool;

    #[test]
    fn test_register_and_lookup() {
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool).unwrap();

        assert_eq!(registry.len(), 1);
        assert!(registry.get("echo").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_duplicate_registration() {
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool).unwrap();
        assert!(registry.register(EchoTool).is_err());
    }

    #[test]
    fn test_list_definitions() {
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool).unwrap();

        let defs = registry.list();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "echo");
    }
}
