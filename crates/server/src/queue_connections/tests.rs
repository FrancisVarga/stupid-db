//! Unit tests for queue connection store.

use crate::credential_store::CredentialStore;

use super::types::*;
use super::QueueConnectionStore;

fn make_input(name: &str) -> QueueConnectionInput {
    QueueConnectionInput {
        name: name.to_string(),
        queue_url: "https://sqs.ap-southeast-1.amazonaws.com/123456789/test-queue".to_string(),
        dlq_url: Some(
            "https://sqs.ap-southeast-1.amazonaws.com/123456789/test-queue-dlq".to_string(),
        ),
        provider: default_provider(),
        enabled: default_enabled(),
        region: default_region(),
        access_key_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
        secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
        session_token: "FwoGZXIvYXdzEBYaDH7example".to_string(),
        endpoint_url: None,
        poll_interval_ms: default_poll_interval_ms(),
        max_batch_size: default_max_batch_size(),
        visibility_timeout_secs: default_visibility_timeout_secs(),
        micro_batch_size: default_micro_batch_size(),
        micro_batch_timeout_ms: default_micro_batch_timeout_ms(),
        color: default_color(),
    }
}

#[test]
fn test_add_and_list() {
    let tmp = tempfile::tempdir().unwrap();
    let store = QueueConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

    let input = make_input("Production Queue");
    let safe = store.add(&input).unwrap();

    assert_eq!(safe.id, "production-queue");
    assert_eq!(safe.access_key_id, "********");
    assert_eq!(safe.secret_access_key, "********");
    assert_eq!(safe.session_token, "********");
    assert_eq!(safe.queue_url, input.queue_url);

    let list = store.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "Production Queue");
    assert_eq!(list[0].access_key_id, "********");
}

#[test]
fn test_get_credentials() {
    let tmp = tempfile::tempdir().unwrap();
    let store = QueueConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

    let input = make_input("Creds Queue");
    store.add(&input).unwrap();

    let creds = store.get_credentials("creds-queue").unwrap().unwrap();
    assert_eq!(creds.access_key_id, "AKIAIOSFODNN7EXAMPLE");
    assert_eq!(
        creds.secret_access_key,
        "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
    );
    assert_eq!(creds.session_token, "FwoGZXIvYXdzEBYaDH7example");
    assert_eq!(creds.queue_url, input.queue_url);
}

#[test]
fn test_update() {
    let tmp = tempfile::tempdir().unwrap();
    let store = QueueConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

    let input = make_input("Update Queue");
    store.add(&input).unwrap();

    let mut updated_input = make_input("Update Queue");
    updated_input.queue_url =
        "https://sqs.us-east-1.amazonaws.com/123456789/new-queue".to_string();
    updated_input.access_key_id = "NEWKEY123".to_string();

    let updated = store
        .update("update-queue", &updated_input)
        .unwrap()
        .unwrap();
    assert_eq!(updated.queue_url, updated_input.queue_url);

    let full = store.get("update-queue").unwrap().unwrap();
    assert_eq!(full.queue_url, updated_input.queue_url);
    assert_eq!(full.access_key_id, "NEWKEY123");
}

#[test]
fn test_update_preserves_credentials() {
    let tmp = tempfile::tempdir().unwrap();
    let store = QueueConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

    let input = make_input("Preserve Queue");
    store.add(&input).unwrap();

    // Update with empty credential fields -- should preserve originals.
    let mut updated_input = make_input("Preserve Queue");
    updated_input.queue_url =
        "https://sqs.us-east-1.amazonaws.com/123456789/changed-queue".to_string();
    updated_input.access_key_id = String::new();
    updated_input.secret_access_key = String::new();
    updated_input.session_token = String::new();

    store
        .update("preserve-queue", &updated_input)
        .unwrap()
        .unwrap();

    let creds = store
        .get_credentials("preserve-queue")
        .unwrap()
        .unwrap();
    assert_eq!(creds.access_key_id, "AKIAIOSFODNN7EXAMPLE");
    assert_eq!(
        creds.secret_access_key,
        "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
    );
    assert_eq!(creds.session_token, "FwoGZXIvYXdzEBYaDH7example");
    assert_eq!(creds.queue_url, updated_input.queue_url);
}

#[test]
fn test_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let store = QueueConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

    let input = make_input("Delete Queue");
    store.add(&input).unwrap();

    assert!(store.delete("delete-queue").unwrap());
    assert!(!store.delete("delete-queue").unwrap());
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn test_duplicate_id() {
    let tmp = tempfile::tempdir().unwrap();
    let store = QueueConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

    let input = make_input("Dupe Queue");
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
