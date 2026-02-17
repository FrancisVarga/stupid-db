use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Permission level for a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionLevel {
    /// Tool executes without asking the user
    AutoApprove,
    /// User must confirm before execution
    RequireConfirmation,
    /// Tool is blocked from executing
    Deny,
}

/// Maps tool names to permission levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionPolicy {
    /// Explicit per-tool permissions
    pub rules: HashMap<String, PermissionLevel>,
    /// Default permission for tools not in the rules map
    pub default: PermissionLevel,
}

impl PermissionPolicy {
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
            default: PermissionLevel::RequireConfirmation,
        }
    }

    /// Get the permission level for a given tool name.
    /// Checks exact match first, then glob patterns, then default.
    pub fn level_for(&self, tool_name: &str) -> PermissionLevel {
        if let Some(&level) = self.rules.get(tool_name) {
            return level;
        }
        // Check glob patterns (e.g., "file_*")
        for (pattern, &level) in &self.rules {
            if pattern.ends_with('*') {
                let prefix = &pattern[..pattern.len() - 1];
                if tool_name.starts_with(prefix) {
                    return level;
                }
            }
        }
        self.default
    }
}

impl Default for PermissionPolicy {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of checking permissions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    Approved,
    Denied(String),
    NeedsConfirmation,
}

/// Trait for checking permissions interactively.
/// The CLI implements this to prompt the user; server mode might auto-decide.
#[async_trait]
pub trait PermissionChecker: Send + Sync {
    async fn check_permission(
        &self,
        tool_name: &str,
        input: &Value,
    ) -> PermissionDecision;
}

/// A simple policy-based permission checker (no interactive prompting).
pub struct PolicyChecker {
    policy: PermissionPolicy,
}

impl PolicyChecker {
    pub fn new(policy: PermissionPolicy) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl PermissionChecker for PolicyChecker {
    async fn check_permission(
        &self,
        tool_name: &str,
        _input: &Value,
    ) -> PermissionDecision {
        match self.policy.level_for(tool_name) {
            PermissionLevel::AutoApprove => PermissionDecision::Approved,
            PermissionLevel::RequireConfirmation => PermissionDecision::NeedsConfirmation,
            PermissionLevel::Deny => {
                PermissionDecision::Denied(format!("Tool '{}' is denied by policy", tool_name))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy() {
        let policy = PermissionPolicy::new();
        assert_eq!(
            policy.level_for("anything"),
            PermissionLevel::RequireConfirmation
        );
    }

    #[test]
    fn test_explicit_rule() {
        let mut policy = PermissionPolicy::new();
        policy
            .rules
            .insert("echo".to_string(), PermissionLevel::AutoApprove);
        assert_eq!(policy.level_for("echo"), PermissionLevel::AutoApprove);
        assert_eq!(
            policy.level_for("other"),
            PermissionLevel::RequireConfirmation
        );
    }

    #[test]
    fn test_glob_pattern() {
        let mut policy = PermissionPolicy::new();
        policy
            .rules
            .insert("file_*".to_string(), PermissionLevel::AutoApprove);
        assert_eq!(
            policy.level_for("file_read"),
            PermissionLevel::AutoApprove
        );
        assert_eq!(
            policy.level_for("file_write"),
            PermissionLevel::AutoApprove
        );
        assert_eq!(
            policy.level_for("bash_execute"),
            PermissionLevel::RequireConfirmation
        );
    }

    #[tokio::test]
    async fn test_policy_checker() {
        let mut policy = PermissionPolicy::new();
        policy
            .rules
            .insert("echo".to_string(), PermissionLevel::AutoApprove);
        policy
            .rules
            .insert("danger".to_string(), PermissionLevel::Deny);

        let checker = PolicyChecker::new(policy);
        assert_eq!(
            checker
                .check_permission("echo", &serde_json::json!({}))
                .await,
            PermissionDecision::Approved
        );
        assert!(matches!(
            checker
                .check_permission("danger", &serde_json::json!({}))
                .await,
            PermissionDecision::Denied(_)
        ));
        assert_eq!(
            checker
                .check_permission("unknown", &serde_json::json!({}))
                .await,
            PermissionDecision::NeedsConfirmation
        );
    }
}
