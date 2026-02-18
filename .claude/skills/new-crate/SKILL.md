---
name: new-crate
description: Scaffold a new crate in the stupid-db Cargo workspace with proper structure and dependencies
disable-model-invocation: true
---

# New Crate — Workspace Scaffolding

Create a new crate in the stupid-db Cargo workspace following project conventions.

## Arguments

The user provides the crate name as the argument. Example: `/new-crate my-crate`

## Steps

1. **Validate name** — Check the crate name doesn't conflict with existing crates in `Cargo.toml` workspace members
2. **Create directory structure**:
   ```
   crates/{name}/
   crates/{name}/src/
   crates/{name}/src/lib.rs
   crates/{name}/Cargo.toml
   ```
3. **Generate `Cargo.toml`** following workspace conventions:
   ```toml
   [package]
   name = "stupid-{name}"
   version.workspace = true
   edition.workspace = true

   [dependencies]
   # Add workspace dependencies as needed
   ```
4. **Generate `src/lib.rs`** with module skeleton:
   ```rust
   //! stupid-{name} — [brief description]

   #[cfg(test)]
   mod tests {
       #[test]
       fn it_works() {
           assert!(true);
       }
   }
   ```
5. **Add to workspace** — Append `"crates/{name}"` to the `members` list in root `Cargo.toml`
6. **Verify** — Run `cargo check -p stupid-{name}` to confirm it compiles

## Conventions

- Crate names use kebab-case: `my-crate` (Cargo package becomes `stupid-my-crate`)
- Use `version.workspace = true` and `edition.workspace = true`
- Reference shared dependencies via `{dep}.workspace = true`
- Use `pub(crate)` for internal helpers — avoid leaking internals
- Add a `#[cfg(test)]` module stub
- Use `thiserror` for error types in library crates
