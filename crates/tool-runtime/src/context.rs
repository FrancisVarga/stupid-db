//! Project context loader.
//!
//! Discovers and loads CLAUDE.md, skills, rules, and agent definitions
//! from a project directory — mimicking Claude Code's auto-discovery.

use std::path::Path;
use tracing::{debug, info};

/// Load project context from a directory, building a system prompt from:
/// - `CLAUDE.md` (project instructions)
/// - `rules/*.md` (behavioral rules)
/// - `skills/*/SKILL.md` (domain knowledge)
/// - `agents/*.md` (agent descriptions — summary only)
///
/// Returns the assembled system prompt string. Returns an empty string
/// if no context files are found.
pub fn load_project_context(dir: &Path) -> String {
    let mut sections: Vec<String> = Vec::new();

    // 1. CLAUDE.md — primary project instructions
    let claude_md = dir.join("CLAUDE.md");
    if claude_md.exists() {
        if let Ok(content) = std::fs::read_to_string(&claude_md) {
            info!(path = %claude_md.display(), "Loaded CLAUDE.md");
            sections.push(content);
        }
    }

    // 2. Rules — behavioral constraints
    let rules_dir = dir.join("rules");
    if rules_dir.is_dir() {
        let mut rule_parts: Vec<String> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&rules_dir) {
            let mut paths: Vec<_> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "md"))
                .collect();
            paths.sort();
            for path in paths {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    debug!(path = %path.display(), "Loaded rule file");
                    rule_parts.push(content);
                }
            }
        }
        if !rule_parts.is_empty() {
            info!(count = rule_parts.len(), "Loaded rule files");
            sections.push(format!(
                "# Additional Rules\n\n{}",
                rule_parts.join("\n\n---\n\n")
            ));
        }
    }

    // 3. Skills — domain knowledge
    let skills_dir = dir.join("skills");
    if skills_dir.is_dir() {
        let mut skill_parts: Vec<String> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&skills_dir) {
            let mut dirs: Vec<_> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            dirs.sort();
            for skill_dir in dirs {
                let skill_file = skill_dir.join("SKILL.md");
                if skill_file.exists() {
                    if let Ok(content) = std::fs::read_to_string(&skill_file) {
                        // Strip YAML frontmatter — keep only the body
                        let body = strip_frontmatter(&content);
                        if !body.trim().is_empty() {
                            debug!(path = %skill_file.display(), "Loaded skill");
                            skill_parts.push(body);
                        }
                    }
                }
            }
        }
        if !skill_parts.is_empty() {
            info!(count = skill_parts.len(), "Loaded skill files");
            sections.push(format!(
                "# Domain Knowledge (Skills)\n\n{}",
                skill_parts.join("\n\n---\n\n")
            ));
        }
    }

    // 4. Agents — brief summary of available agents
    let agents_dir = dir.join("agents");
    if agents_dir.is_dir() {
        let mut agent_summaries: Vec<String> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&agents_dir) {
            let mut paths: Vec<_> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "md"))
                .collect();
            paths.sort();
            for path in paths {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    // Extract name and description from frontmatter
                    if let Some(summary) = extract_agent_summary(&content) {
                        agent_summaries.push(summary);
                    }
                }
            }
        }
        if !agent_summaries.is_empty() {
            info!(count = agent_summaries.len(), "Loaded agent summaries");
            sections.push(format!(
                "# Available Agents\n\n{}",
                agent_summaries.join("\n")
            ));
        }
    }

    if sections.is_empty() {
        debug!(dir = %dir.display(), "No project context files found");
        String::new()
    } else {
        sections.join("\n\n---\n\n")
    }
}

/// Strip YAML frontmatter (between `---` delimiters) from markdown content.
fn strip_frontmatter(content: &str) -> String {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return content.to_string();
    }
    // Find second --- delimiter
    if let Some(end) = trimmed[3..].find("\n---") {
        let after = &trimmed[3 + end + 4..]; // skip past "\n---"
        after.trim_start_matches('\n').to_string()
    } else {
        content.to_string()
    }
}

/// Extract a one-line summary from agent frontmatter (name + description).
fn extract_agent_summary(content: &str) -> Option<String> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let end = trimmed[3..].find("\n---")?;
    let frontmatter = &trimmed[3..3 + end];

    let mut name = None;
    let mut description = None;
    for line in frontmatter.lines() {
        if let Some(val) = line.strip_prefix("name:") {
            name = Some(val.trim().to_string());
        }
        if let Some(val) = line.strip_prefix("description:") {
            description = Some(val.trim().to_string());
        }
    }

    match (name, description) {
        (Some(n), Some(d)) => Some(format!("- **{}**: {}", n, d)),
        (Some(n), None) => Some(format!("- **{}**", n)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_frontmatter() {
        let input = "---\nname: test\n---\n# Body\nContent here";
        assert_eq!(strip_frontmatter(input), "# Body\nContent here");
    }

    #[test]
    fn test_strip_frontmatter_no_frontmatter() {
        let input = "# Just markdown\nNo frontmatter";
        assert_eq!(strip_frontmatter(input), input);
    }

    #[test]
    fn test_extract_agent_summary() {
        let input = "---\nname: rust-dev\ndescription: Rust backend specialist\ntools:\n  - Read\n---\n# Body";
        assert_eq!(
            extract_agent_summary(input),
            Some("- **rust-dev**: Rust backend specialist".to_string())
        );
    }

    #[test]
    fn test_extract_agent_summary_no_frontmatter() {
        assert_eq!(extract_agent_summary("# No frontmatter"), None);
    }
}
