# stupid-claude-agent — Project-Specific AI Team Plugin

## Plugin Overview

This plugin provides a hierarchical team of 7 AI agents and 13 skills deeply specialized for the stupid-db knowledge materialization engine. Agents understand the 12-crate Cargo workspace, Next.js dashboard, w88 gaming data domain, and full operational stack.

## Team Hierarchy

- **Tier 1**: `architect` — full system design, cross-cutting review, delegation
- **Tier 2**: `backend-lead`, `frontend-lead`, `data-lead` — domain coordination
- **Tier 3**: `compute-specialist`, `ingest-specialist`, `query-specialist` — deep domain expertise

## When to Use Which Agent

| Task | Agent |
|------|-------|
| Architecture decisions, ADRs, cross-crate changes | `architect` |
| Any Rust crate work (general) | `backend-lead` |
| Algorithm implementation, scheduler tuning | `compute-specialist` |
| Data pipeline, entity extraction, embedding | `ingest-specialist` |
| Query plans, LLM prompts, catalog | `query-specialist` |
| Dashboard UI, D3 charts, chat interface | `frontend-lead` |
| Data analysis, OpenSearch, domain questions | `data-lead` |

## Rules

- All agents inherit root CLAUDE.md behavioral rules
- Never modify D:\w88_data — read-only production sample data
- Use tracing for logging, never println!
- Use thiserror in library crates, anyhow in server binary
- Dashboard is chat-first, D3.js only, no auth
- Query interface supports OpenAI, Claude, and Ollama — never hardcode single provider
