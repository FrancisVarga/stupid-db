//! Unit tests for Athena connection store.

use crate::credential_store::CredentialStore;

use super::types::*;
use super::AthenaConnectionStore;

fn make_input(name: &str) -> AthenaConnectionInput {
    AthenaConnectionInput {
        name: name.to_string(),
        region: default_region(),
        catalog: default_catalog(),
        database: "analytics_db".to_string(),
        workgroup: default_workgroup(),
        output_location: "s3://my-athena-results/output/".to_string(),
        access_key_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
        secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
        session_token: "FwoGZXIvYXdzEBYaDH7example".to_string(),
        endpoint_url: None,
        enabled: default_enabled(),
        color: default_color(),
    }
}

#[test]
fn test_add_and_list() {
    let tmp = tempfile::tempdir().unwrap();
    let store = AthenaConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

    let input = make_input("Production Athena");
    let safe = store.add(&input).unwrap();

    assert_eq!(safe.id, "production-athena");
    assert_eq!(safe.access_key_id, "********");
    assert_eq!(safe.secret_access_key, "********");
    assert_eq!(safe.session_token, "********");
    assert_eq!(safe.database, "analytics_db");
    assert_eq!(safe.output_location, "s3://my-athena-results/output/");
    assert_eq!(safe.schema_status, "pending");
    assert!(safe.schema.is_none());

    let list = store.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "Production Athena");
    assert_eq!(list[0].access_key_id, "********");
}

#[test]
fn test_get_credentials() {
    let tmp = tempfile::tempdir().unwrap();
    let store = AthenaConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

    let input = make_input("Creds Athena");
    store.add(&input).unwrap();

    let creds = store.get_credentials("creds-athena").unwrap().unwrap();
    assert_eq!(creds.access_key_id, "AKIAIOSFODNN7EXAMPLE");
    assert_eq!(
        creds.secret_access_key,
        "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
    );
    assert_eq!(creds.session_token, "FwoGZXIvYXdzEBYaDH7example");
    assert_eq!(creds.database, "analytics_db");
    assert_eq!(creds.region, "ap-southeast-1");
    assert_eq!(creds.catalog, "AwsDataCatalog");
    assert_eq!(creds.workgroup, "primary");
}

#[test]
fn test_update() {
    let tmp = tempfile::tempdir().unwrap();
    let store = AthenaConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

    let input = make_input("Update Athena");
    store.add(&input).unwrap();

    let mut updated_input = make_input("Update Athena");
    updated_input.database = "new_analytics_db".to_string();
    updated_input.access_key_id = "NEWKEY123".to_string();

    // Empty credentials should preserve the originals.
    updated_input.secret_access_key = String::new();
    updated_input.session_token = String::new();

    let updated = store
        .update("update-athena", &updated_input)
        .unwrap()
        .unwrap();
    assert_eq!(updated.database, "new_analytics_db");

    let creds = store
        .get_credentials("update-athena")
        .unwrap()
        .unwrap();
    assert_eq!(creds.database, "new_analytics_db");
    assert_eq!(creds.access_key_id, "NEWKEY123");
    // Preserved from original.
    assert_eq!(
        creds.secret_access_key,
        "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
    );
    assert_eq!(creds.session_token, "FwoGZXIvYXdzEBYaDH7example");
}

#[test]
fn test_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let store = AthenaConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

    let input = make_input("Delete Athena");
    store.add(&input).unwrap();

    assert!(store.delete("delete-athena").unwrap());
    assert!(!store.delete("delete-athena").unwrap());
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn test_duplicate_id() {
    let tmp = tempfile::tempdir().unwrap();
    let store = AthenaConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

    let input = make_input("Dupe Athena");
    store.add(&input).unwrap();

    let result = store.add(&input);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("already exists")
    );
}

#[test]
fn test_schema_update() {
    let tmp = tempfile::tempdir().unwrap();
    let store = AthenaConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

    let input = make_input("Schema Athena");
    store.add(&input).unwrap();

    // Verify initial status.
    let config = store.get("schema-athena").unwrap().unwrap();
    assert_eq!(config.schema_status, "pending");
    assert!(config.schema.is_none());

    // Update status to "fetching".
    assert!(store
        .update_schema_status("schema-athena", "fetching")
        .unwrap());
    let config = store.get("schema-athena").unwrap().unwrap();
    assert_eq!(config.schema_status, "fetching");

    // Set full schema.
    let schema = AthenaSchema {
        databases: vec![AthenaDatabase {
            name: "analytics_db".to_string(),
            tables: vec![AthenaTable {
                name: "events".to_string(),
                columns: vec![
                    AthenaColumn {
                        name: "event_id".to_string(),
                        data_type: "string".to_string(),
                        comment: Some("Unique event identifier".to_string()),
                    },
                    AthenaColumn {
                        name: "timestamp".to_string(),
                        data_type: "timestamp".to_string(),
                        comment: None,
                    },
                ],
            }],
        }],
        fetched_at: chrono::Utc::now().to_rfc3339(),
    };

    assert!(store.update_schema("schema-athena", schema).unwrap());

    let config = store.get("schema-athena").unwrap().unwrap();
    assert_eq!(config.schema_status, "ready");
    let schema = config.schema.unwrap();
    assert_eq!(schema.databases.len(), 1);
    assert_eq!(schema.databases[0].name, "analytics_db");
    assert_eq!(schema.databases[0].tables.len(), 1);
    assert_eq!(schema.databases[0].tables[0].name, "events");
    assert_eq!(schema.databases[0].tables[0].columns.len(), 2);

    // Non-existent ID returns false.
    assert!(!store
        .update_schema_status("nonexistent", "failed")
        .unwrap());
}
