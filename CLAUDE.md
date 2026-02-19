# stupid-db — Project Rules

## Behavioral Rules (Always Enforced)

- Add under a ## Code Navigation section in CLAUDE.md\n\nWhen exploring code, prefer LSP-based tools over grep/glob when available. User has expressed a preference for LSP-first analysis.
- When creating many github issues using gh cli batch them with & its much faster than creating them one by one. For example: `gh issue create -t "Issue Title" -b "Issue Body" &` (repeat for each issue)
- ALWAYS prefer editing an existing file to creating a new one
- NEVER proactively create documentation files (\*.md) or README files unless explicitly requested
- NEVER save working files, text/mds, or tests to the root folder
- ALWAYS read a file before editing it
- NEVER commit secrets, credentials, or .env files
- ALWAYS batch ALL file reads/writes/edits in ONE message
- ALWAYS batch ALL terminal operations in ONE Bash message (note: if one parallel call fails, siblings may cascade-fail — re-run unfailed queries in next batch)
- When modifying file paths or moving files, always fix ALL cross-references and imports across the entire codebase. Never create copies of files as a workaround — fix the actual path references instead.
- ALWAYS store project-related memories in `.claude/memory/` folder — not in project root or other locations

## Data Safety

- Never modify D:\w88_data — treat as read-only production sample data

## Architecture Patterns

- Mirror existing CRUD patterns when adding new resource types — same file structure, naming, API routes
- Form → Next.js proxy → Rust backend (encrypted JSON) → Dashboard is standard 4-layer data flow

## Design Workflow

- For new features, propose architecture in conversation before writing docs or code
- Update docs before implementing features — architecture stabilizes through writing
- Document ADRs in docs/architecture/decisions/ before major architectural changes

## Windows Development

- On Windows, always check system PATH and environment variables early when debugging tool/binary resolution issues. Prefer modifying system PATH over shims, symlinks, or wrapper scripts.

## Troubleshooting

- When stuck, consult git history (commits, PRs) and .claude/retrospectives/ for context and past solutions
