use std::collections::HashMap;

use super::helpers::{parse_endpoint_to_transport, topological_sort};
use super::types::{EisenbahnConfig, StageConfig};

#[test]
fn parse_minimal_toml() {
    let toml = r#"
[broker]
frontend = "ipc:///tmp/stupid-db/broker-fe.sock"
backend = "ipc:///tmp/stupid-db/broker-be.sock"
"#;
    let cfg = EisenbahnConfig::from_toml(toml).unwrap();
    assert!(cfg.broker.frontend.contains("broker-fe"));
    assert!(cfg.broker.backend.contains("broker-be"));
    assert!(cfg.workers.is_empty());
}

#[test]
fn parse_full_toml() {
    let toml = r#"
[broker]
frontend = "tcp://10.0.0.1:5555"
backend = "tcp://10.0.0.1:5556"
metrics_port = 9090

[transport]
kind = "tcp"
default_host = "10.0.0.1"
base_port = 5560

[workers.ingest]
binary = "stupid-ingest"
subscriptions = ["raw.parquet"]
pipelines = ["ingest"]
instances = 2

[workers.compute]
binary = "stupid-compute"
subscriptions = ["entity.created"]
pipelines = ["compute"]

[pipeline.stages.ingest]
concurrency = 4

[pipeline.stages.compute]
after = ["ingest"]
concurrency = 2

[pipeline.stages.graph]
after = ["compute"]
endpoint = "tcp://10.0.0.1:5570"
"#;
    let cfg = EisenbahnConfig::from_toml(toml).unwrap();
    assert_eq!(cfg.broker.metrics_port, Some(9090));
    assert_eq!(cfg.workers.len(), 2);
    assert_eq!(cfg.workers["ingest"].instances, 2);
    assert_eq!(cfg.workers["compute"].instances, 1); // default
    assert_eq!(cfg.pipeline.stages.len(), 3);

    let order = cfg.pipeline_order().unwrap();
    let ingest_pos = order.iter().position(|s| s == "ingest").unwrap();
    let compute_pos = order.iter().position(|s| s == "compute").unwrap();
    let graph_pos = order.iter().position(|s| s == "graph").unwrap();
    assert!(ingest_pos < compute_pos);
    assert!(compute_pos < graph_pos);
}

#[test]
fn detect_circular_dependency() {
    let toml = r#"
[broker]

[pipeline.stages.a]
after = ["c"]

[pipeline.stages.b]
after = ["a"]

[pipeline.stages.c]
after = ["b"]
"#;
    let err = EisenbahnConfig::from_toml(toml).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("circular") || msg.contains("cycle"),
        "got: {msg}"
    );
}

#[test]
fn detect_missing_upstream_reference() {
    let toml = r#"
[broker]

[pipeline.stages.compute]
after = ["nonexistent"]
"#;
    let err = EisenbahnConfig::from_toml(toml).unwrap_err();
    assert!(err.to_string().contains("nonexistent"));
}

#[test]
fn detect_worker_bad_pipeline_ref() {
    let toml = r#"
[broker]

[pipeline.stages.ingest]

[workers.bad]
binary = "bad-worker"
pipelines = ["missing_stage"]
"#;
    let err = EisenbahnConfig::from_toml(toml).unwrap_err();
    assert!(err.to_string().contains("missing_stage"));
}

#[test]
fn detect_invalid_transport_kind() {
    let toml = r#"
[broker]

[transport]
kind = "udp"
"#;
    let err = EisenbahnConfig::from_toml(toml).unwrap_err();
    assert!(err.to_string().contains("udp"));
}

#[test]
fn env_override_broker_frontend() {
    // SAFETY: test-only, nextest runs each test in its own process
    unsafe {
        std::env::set_var("EISENBAHN_BROKER_FRONTEND", "tcp://override:9999");
    }
    let toml = r#"
[broker]
frontend = "ipc:///tmp/original.sock"
backend = "ipc:///tmp/backend.sock"
"#;
    let cfg = EisenbahnConfig::from_toml(toml).unwrap();
    assert_eq!(cfg.broker.frontend, "tcp://override:9999");
    unsafe {
        std::env::remove_var("EISENBAHN_BROKER_FRONTEND");
    }
}

#[test]
fn env_override_transport_base_port() {
    // SAFETY: test-only, nextest runs each test in its own process
    unsafe {
        std::env::set_var("EISENBAHN_TRANSPORT_BASE_PORT", "7777");
    }
    let toml = "[broker]\n";
    let cfg = EisenbahnConfig::from_toml(toml).unwrap();
    assert_eq!(cfg.transport.base_port, 7777);
    unsafe {
        std::env::remove_var("EISENBAHN_TRANSPORT_BASE_PORT");
    }
}

#[test]
fn local_config_defaults() {
    let cfg = EisenbahnConfig::local();
    assert!(cfg.broker.frontend.contains("ipc://"));
    assert_eq!(cfg.transport.kind, "ipc");
    assert!(cfg.workers.is_empty());
}

#[test]
fn distributed_config() {
    let cfg = EisenbahnConfig::distributed("10.0.0.1", 5555);
    assert_eq!(cfg.broker.frontend, "tcp://10.0.0.1:5555");
    assert_eq!(cfg.broker.backend, "tcp://10.0.0.1:5556");
    assert_eq!(cfg.broker.metrics_port, Some(5557));
    assert_eq!(cfg.transport.kind, "tcp");
}

#[test]
fn broker_transport_resolution() {
    let cfg = EisenbahnConfig::distributed("10.0.0.1", 5555);
    let frontend = cfg.broker_frontend_transport();
    assert_eq!(frontend.endpoint(), "tcp://10.0.0.1:5555");
}

#[test]
fn empty_pipeline_is_valid() {
    let toml = "[broker]\n";
    let cfg = EisenbahnConfig::from_toml(toml).unwrap();
    assert!(cfg.pipeline_order().unwrap().is_empty());
}

#[test]
fn worker_env_vars() {
    let toml = r#"
[broker]

[workers.compute]
binary = "stupid-compute"

[workers.compute.env]
RUST_LOG = "debug"
CUDA_VISIBLE_DEVICES = "0,1"
"#;
    let cfg = EisenbahnConfig::from_toml(toml).unwrap();
    let w = &cfg.workers["compute"];
    assert_eq!(w.env["RUST_LOG"], "debug");
    assert_eq!(w.env["CUDA_VISIBLE_DEVICES"], "0,1");
}

#[test]
fn parse_endpoint_ipc() {
    let t = parse_endpoint_to_transport("ipc:///tmp/stupid-db/broker-frontend.sock");
    assert_eq!(t.endpoint(), "ipc:///tmp/stupid-db/broker-frontend.sock");
}

#[test]
fn parse_endpoint_tcp() {
    let t = parse_endpoint_to_transport("tcp://10.0.0.1:5555");
    assert_eq!(t.endpoint(), "tcp://10.0.0.1:5555");
}

#[test]
fn topological_sort_linear() {
    let mut stages = HashMap::new();
    stages.insert(
        "a".into(),
        StageConfig { after: vec![], endpoint: None, concurrency: 1 },
    );
    stages.insert(
        "b".into(),
        StageConfig { after: vec!["a".into()], endpoint: None, concurrency: 1 },
    );
    stages.insert(
        "c".into(),
        StageConfig { after: vec!["b".into()], endpoint: None, concurrency: 1 },
    );

    let order = topological_sort(&stages).unwrap();
    let pos = |s: &str| order.iter().position(|x| x == s).unwrap();
    assert!(pos("a") < pos("b"));
    assert!(pos("b") < pos("c"));
}

#[test]
fn topological_sort_diamond() {
    let mut stages = HashMap::new();
    stages.insert(
        "a".into(),
        StageConfig { after: vec![], endpoint: None, concurrency: 1 },
    );
    stages.insert(
        "b".into(),
        StageConfig { after: vec!["a".into()], endpoint: None, concurrency: 1 },
    );
    stages.insert(
        "c".into(),
        StageConfig { after: vec!["a".into()], endpoint: None, concurrency: 1 },
    );
    stages.insert(
        "d".into(),
        StageConfig {
            after: vec!["b".into(), "c".into()],
            endpoint: None,
            concurrency: 1,
        },
    );

    let order = topological_sort(&stages).unwrap();
    let pos = |s: &str| order.iter().position(|x| x == s).unwrap();
    assert!(pos("a") < pos("b"));
    assert!(pos("a") < pos("c"));
    assert!(pos("b") < pos("d"));
    assert!(pos("c") < pos("d"));
}

#[test]
fn self_referencing_stage_is_circular() {
    let toml = r#"
[broker]

[pipeline.stages.loop]
after = ["loop"]
"#;
    let err = EisenbahnConfig::from_toml(toml).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("circular") || msg.contains("cycle"),
        "got: {msg}"
    );
}
