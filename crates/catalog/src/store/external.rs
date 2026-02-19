use crate::catalog::{ExternalDatabase, ExternalSource, ExternalTable};

use super::error::CatalogStoreError;
use super::CatalogStore;

impl CatalogStore {
    // ── External sources (SQL databases like Athena) ────────────

    /// Save an external source schema in a hierarchical structure.
    ///
    /// Directory structure:
    /// ```text
    /// external/{kind}-{connection_id}/
    ///   metadata.json           <- name, kind, connection_id
    ///   {database}/
    ///     {table}.json          <- table schema (name, columns)
    /// ```
    ///
    /// Example: `external/athena-prod-lake/analytics/events.json`
    pub fn save_external_source(
        &self,
        source: &ExternalSource,
    ) -> Result<(), CatalogStoreError> {
        let source_dir = self.base_dir
            .join("external")
            .join(format!("{}-{}", source.kind, source.connection_id));

        // Save metadata
        let metadata = serde_json::json!({
            "name": source.name,
            "kind": source.kind,
            "connection_id": source.connection_id,
        });
        let metadata_path = source_dir.join("metadata.json");
        std::fs::create_dir_all(&source_dir)?;
        std::fs::write(metadata_path, serde_json::to_string_pretty(&metadata)?)?;

        // Save each database's tables
        for db in &source.databases {
            let db_dir = source_dir.join(&db.name);
            std::fs::create_dir_all(&db_dir)?;

            for table in &db.tables {
                let table_path = db_dir.join(format!("{}.json", table.name));
                let json = serde_json::to_string_pretty(table)?;
                std::fs::write(table_path, json)?;
            }
        }

        Ok(())
    }

    /// Load an external source by kind and connection ID.
    pub fn load_external_source(
        &self,
        kind: &str,
        connection_id: &str,
    ) -> Result<Option<ExternalSource>, CatalogStoreError> {
        let source_dir = self.base_dir
            .join("external")
            .join(format!("{}-{}", kind, connection_id));

        if !source_dir.exists() {
            return Ok(None);
        }

        // Load metadata
        let metadata_path = source_dir.join("metadata.json");
        let metadata: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(metadata_path)?)?;

        let name = metadata["name"].as_str().unwrap_or("").to_string();
        let kind = metadata["kind"].as_str().unwrap_or("").to_string();
        let connection_id = metadata["connection_id"].as_str().unwrap_or("").to_string();

        // Load all databases
        let mut databases = Vec::new();
        for db_entry in std::fs::read_dir(&source_dir)? {
            let db_entry = db_entry?;
            let db_path = db_entry.path();

            if db_path.is_dir() {
                let db_name = db_path.file_name().unwrap().to_string_lossy().to_string();
                let mut tables = Vec::new();

                // Load all tables in this database
                for table_entry in std::fs::read_dir(&db_path)? {
                    let table_entry = table_entry?;
                    let table_path = table_entry.path();

                    if table_path.extension().and_then(|s| s.to_str()) == Some("json") {
                        let json = std::fs::read_to_string(&table_path)?;
                        let table: ExternalTable = serde_json::from_str(&json)?;
                        tables.push(table);
                    }
                }

                tables.sort_by(|a, b| a.name.cmp(&b.name));
                databases.push(ExternalDatabase {
                    name: db_name,
                    tables,
                });
            }
        }

        databases.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(Some(ExternalSource {
            name,
            kind,
            connection_id,
            databases,
        }))
    }

    /// List all external sources on disk.
    pub fn list_external_sources(&self) -> Result<Vec<ExternalSource>, CatalogStoreError> {
        let external_dir = self.base_dir.join("external");
        let mut sources = Vec::new();

        for entry in std::fs::read_dir(external_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Extract kind and connection_id from directory name: "{kind}-{connection_id}"
                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                    if let Some(dash_pos) = dir_name.find('-') {
                        let kind = &dir_name[..dash_pos];
                        let connection_id = &dir_name[dash_pos + 1..];

                        if let Ok(Some(source)) = self.load_external_source(kind, connection_id) {
                            sources.push(source);
                        }
                    }
                }
            }
        }

        sources.sort_by(|a, b| a.connection_id.cmp(&b.connection_id));
        Ok(sources)
    }

    /// Remove an external source by kind and connection ID.
    pub fn remove_external_source(
        &self,
        kind: &str,
        connection_id: &str,
    ) -> Result<(), CatalogStoreError> {
        let source_dir = self.base_dir
            .join("external")
            .join(format!("{}-{}", kind, connection_id));

        if source_dir.exists() {
            std::fs::remove_dir_all(source_dir)?;
        }
        Ok(())
    }
}
