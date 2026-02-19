//! migrate_skills — Extract embedded agent skills to standalone YAML files.
//!
//! Scans agent YAML files, extracts embedded `skills` entries into
//! `data/bundeswehr/skills/*.yml`, and updates agents to use `skill_refs`.
//!
//! Usage:
//!   cargo run --bin migrate-skills -- --agents-dir data/agents --skills-dir data/bundeswehr/skills
//!   cargo run --bin migrate-skills -- --dry-run   # preview without writing

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;

use stupid_agent::yaml_schema::{AgentYamlConfig, SkillYamlConfig};

/// Migrate embedded agent skills to standalone skill files.
#[derive(Parser)]
#[command(name = "migrate-skills")]
struct Args {
    /// Directory containing agent YAML files.
    #[arg(long, default_value = "data/agents")]
    agents_dir: PathBuf,

    /// Directory where standalone skill files will be written.
    #[arg(long, default_value = "data/bundeswehr/skills")]
    skills_dir: PathBuf,

    /// Log file path for migration actions.
    #[arg(long, default_value = "data/bundeswehr/migration.log")]
    log_file: PathBuf,

    /// Preview changes without writing anything.
    #[arg(long)]
    dry_run: bool,
}

struct MigrationLog {
    entries: Vec<String>,
    file: Option<std::fs::File>,
}

impl MigrationLog {
    fn new(path: &Path, dry_run: bool) -> Result<Self> {
        let file = if dry_run {
            None
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            Some(
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .with_context(|| format!("failed to open log file: {}", path.display()))?,
            )
        };
        Ok(Self {
            entries: Vec::new(),
            file,
        })
    }

    fn log(&mut self, action: &str, file: &str, details: &str) {
        let ts = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
        let line = format!("[{ts}] {action} {file} - {details}");
        eprintln!("{line}");
        self.entries.push(line.clone());
        if let Some(ref mut f) = self.file {
            let _ = writeln!(f, "{line}");
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.dry_run {
        eprintln!("=== DRY RUN — no files will be written ===\n");
    }

    let mut log = MigrationLog::new(&args.log_file, args.dry_run)?;

    // Ensure skills directory exists (unless dry run).
    if !args.dry_run {
        std::fs::create_dir_all(&args.skills_dir)
            .with_context(|| format!("failed to create skills dir: {}", args.skills_dir.display()))?;
    }

    // Load existing standalone skills (for collision detection).
    let mut existing_skills: HashMap<String, SkillYamlConfig> = HashMap::new();
    if args.skills_dir.exists() {
        for entry in std::fs::read_dir(&args.skills_dir)?.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str());
            if !matches!(ext, Some("yaml" | "yml")) {
                continue;
            }
            if let Ok(skill) = load_skill(&path) {
                existing_skills.insert(skill.name.clone(), skill);
            }
        }
    }

    // Scan agent files.
    if !args.agents_dir.exists() {
        anyhow::bail!("Agents directory not found: {}", args.agents_dir.display());
    }

    let mut agents_modified = 0usize;
    let mut skills_extracted = 0usize;
    let mut skills_skipped = 0usize;

    let mut entries: Vec<_> = std::fs::read_dir(&args.agents_dir)?
        .flatten()
        .collect();
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str());
        if !matches!(ext, Some("yaml" | "yml")) {
            continue;
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read: {}", path.display()))?;

        let mut agent: AgentYamlConfig = match serde_yaml::from_str(&content) {
            Ok(a) => a,
            Err(e) => {
                log.log("SKIP", &path.display().to_string(), &format!("parse error: {e}"));
                continue;
            }
        };

        if agent.skills.is_empty() {
            continue;
        }

        let mut new_refs: Vec<String> = agent.skill_refs.clone();
        let mut extracted_any = false;

        for skill in &agent.skills {
            let mut skill_name = skill.name.clone();

            // Check for collision with existing standalone skills.
            if let Some(existing) = existing_skills.get(&skill_name) {
                if existing.prompt == skill.prompt {
                    // Same skill already extracted — just add ref.
                    if !new_refs.contains(&skill_name) {
                        new_refs.push(skill_name.clone());
                        log.log(
                            "REUSE",
                            &skill_name,
                            &format!("identical skill already exists, adding ref in {}", agent.name),
                        );
                    } else {
                        log.log("SKIP", &skill_name, "already referenced by agent");
                        skills_skipped += 1;
                    }
                    continue;
                } else {
                    // Different prompt — append suffix.
                    skill_name = format!("{skill_name}-v2");
                    log.log(
                        "RENAME",
                        &skill.name,
                        &format!("collision (different prompt), using '{skill_name}'"),
                    );
                }
            }

            // Write standalone skill file.
            let standalone = SkillYamlConfig {
                name: skill_name.clone(),
                description: String::new(),
                prompt: skill.prompt.clone(),
                tags: Vec::new(),
                version: "1.0.0".to_string(),
            };

            let skill_path = args.skills_dir.join(format!("{skill_name}.yml"));

            if !args.dry_run {
                let yaml = serde_yaml::to_string(&standalone)
                    .with_context(|| format!("failed to serialize skill: {skill_name}"))?;
                std::fs::write(&skill_path, &yaml)
                    .with_context(|| format!("failed to write: {}", skill_path.display()))?;
            }

            log.log(
                "EXTRACT",
                &skill_path.display().to_string(),
                &format!("from agent '{}', skill '{}'", agent.name, skill.name),
            );

            existing_skills.insert(skill_name.clone(), standalone);

            if !new_refs.contains(&skill_name) {
                new_refs.push(skill_name);
            }
            skills_extracted += 1;
            extracted_any = true;
        }

        if extracted_any || new_refs != agent.skill_refs {
            // Update agent: clear embedded skills, set skill_refs.
            agent.skills.clear();
            agent.skill_refs = new_refs;

            if !args.dry_run {
                let yaml = serde_yaml::to_string(&agent)
                    .with_context(|| format!("failed to serialize agent: {}", agent.name))?;
                std::fs::write(&path, &yaml)
                    .with_context(|| format!("failed to write: {}", path.display()))?;
            }

            log.log(
                "UPDATE",
                &path.display().to_string(),
                &format!(
                    "agent '{}': cleared {} embedded skills, set {} skill_refs",
                    agent.name,
                    agent.skills.len(),
                    agent.skill_refs.len(),
                ),
            );
            agents_modified += 1;
        }
    }

    eprintln!();
    eprintln!("=== Migration {} ===", if args.dry_run { "preview" } else { "complete" });
    eprintln!("  Agents modified:  {agents_modified}");
    eprintln!("  Skills extracted: {skills_extracted}");
    eprintln!("  Skills skipped:   {skills_skipped}");

    if !args.dry_run {
        eprintln!("  Log written to:   {}", args.log_file.display());
    }

    Ok(())
}

/// Load a single standalone skill from a YAML file.
fn load_skill(path: &Path) -> Result<SkillYamlConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read: {}", path.display()))?;
    let config: SkillYamlConfig = serde_yaml::from_str(&content)
        .with_context(|| format!("YAML parse error in {}", path.display()))?;
    Ok(config)
}
