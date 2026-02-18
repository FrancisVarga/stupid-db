//! Service request/response message payloads for DEALER/ROUTER routing.
//!
//! These types represent typed RPC-style interactions between API handlers
//! and service workers. Each service has a request type and a response type,
//! serialized with MessagePack via [`Message::new`](crate::Message::new).

use serde::{Deserialize, Serialize};

// ─── Query Service ─────────────────────────────────────────────────────────

/// Request to execute a natural-language query against the knowledge engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryServiceRequest {
    /// The natural-language question to answer.
    pub question: String,
}

/// Response containing the query plan and result set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryServiceResponse {
    /// The execution plan produced by the query planner.
    pub plan: serde_json::Value,
    /// Result rows as JSON objects.
    pub results: Vec<serde_json::Value>,
}

// ─── Agent Service ─────────────────────────────────────────────────────────

/// Request to invoke an AI agent for task execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum AgentServiceRequest {
    /// Execute a named agent with a task description.
    Execute {
        agent_name: String,
        task: String,
        context: serde_json::Value,
    },
    /// Execute a named agent with conversation history.
    ExecuteWithHistory {
        agent_name: String,
        task: String,
        history: Vec<serde_json::Value>,
        context: serde_json::Value,
        max_history: usize,
    },
    /// Execute directly without a named agent (ad-hoc).
    ExecuteDirect {
        task: String,
        history: Vec<serde_json::Value>,
        context: serde_json::Value,
        max_history: usize,
    },
    /// Execute a task across a team of agents.
    TeamExecute {
        task: String,
        strategy: String,
        context: serde_json::Value,
    },
}

/// Response from an agent execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentServiceResponse {
    /// The agent's output text.
    pub output: String,
    /// Execution status (e.g. "success", "error", "timeout").
    pub status: String,
    /// Wall-clock execution time in milliseconds.
    pub elapsed_ms: u64,
    /// Optional per-agent outputs when using team execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_outputs: Option<Vec<serde_json::Value>>,
}

// ─── Athena Service ────────────────────────────────────────────────────────

/// Request to execute queries against AWS Athena.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum AthenaServiceRequest {
    /// Execute a SQL query and return JSON rows.
    Query {
        connection_id: String,
        sql: String,
        database: Option<String>,
    },
    /// Execute a SQL query and return Parquet bytes.
    QueryParquet {
        connection_id: String,
        sql: String,
        database: Option<String>,
    },
    /// Refresh the cached schema for a connection.
    SchemaRefresh { connection_id: String },
}

/// Streaming response chunk from an Athena query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum AthenaServiceResponse {
    /// Query execution status update.
    Status {
        state: String,
        stats: Option<serde_json::Value>,
    },
    /// Column metadata for the result set.
    Columns { columns: Vec<AthenaColumnInfo> },
    /// A batch of result rows.
    Rows { rows: Vec<Vec<serde_json::Value>> },
    /// Raw Parquet-encoded result data.
    Parquet { data: Vec<u8> },
    /// Schema refresh completed successfully.
    SchemaRefreshed { status: String },
    /// Terminal chunk indicating the query is complete.
    Done { total_rows: Option<u64> },
    /// Terminal chunk indicating an error occurred.
    Error { message: String },
}

/// Column metadata returned by Athena queries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AthenaColumnInfo {
    /// Column name.
    pub name: String,
    /// Athena data type (e.g. "varchar", "bigint", "double").
    pub data_type: String,
}

// ─── Catalog Query Service ─────────────────────────────────────────────────

/// Request to execute a multi-step catalog query plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogQueryRequest {
    /// Ordered query plan steps to execute.
    pub steps: Vec<serde_json::Value>,
}

/// Response containing catalog query results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogQueryResponse {
    /// Results from each plan step.
    pub results: Vec<serde_json::Value>,
}

// ─── Generic Service Error ─────────────────────────────────────────────────

/// Returned when a service worker encounters an error processing a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceError {
    /// HTTP-style status code (e.g. 400, 404, 500).
    pub code: u16,
    /// Human-readable error description.
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip<T>(val: &T) -> T
    where
        T: Serialize + for<'de> Deserialize<'de> + std::fmt::Debug + PartialEq,
    {
        let bytes = rmp_serde::to_vec(val).expect("serialize");
        rmp_serde::from_slice(&bytes).expect("deserialize")
    }

    #[test]
    fn roundtrip_query_request() {
        let msg = QueryServiceRequest {
            question: "What are the top anomalies today?".into(),
        };
        assert_eq!(roundtrip(&msg), msg);
    }

    #[test]
    fn roundtrip_query_response() {
        let msg = QueryServiceResponse {
            plan: serde_json::json!({"steps": ["scan", "filter"]}),
            results: vec![serde_json::json!({"entity": "user-1", "score": 0.95})],
        };
        assert_eq!(roundtrip(&msg), msg);
    }

    #[test]
    fn roundtrip_agent_request_variants() {
        let variants: Vec<AgentServiceRequest> = vec![
            AgentServiceRequest::Execute {
                agent_name: "analyst".into(),
                task: "Summarize anomalies".into(),
                context: serde_json::json!({}),
            },
            AgentServiceRequest::ExecuteWithHistory {
                agent_name: "analyst".into(),
                task: "Follow up".into(),
                history: vec![serde_json::json!({"role": "user", "content": "hi"})],
                context: serde_json::json!({"session": "abc"}),
                max_history: 10,
            },
            AgentServiceRequest::ExecuteDirect {
                task: "Quick answer".into(),
                history: vec![],
                context: serde_json::json!(null),
                max_history: 5,
            },
            AgentServiceRequest::TeamExecute {
                task: "Analyze dataset".into(),
                strategy: "fan-out".into(),
                context: serde_json::json!({"dataset": "sales"}),
            },
        ];
        for v in &variants {
            assert_eq!(roundtrip(v), *v);
        }
    }

    #[test]
    fn roundtrip_agent_response() {
        let msg = AgentServiceResponse {
            output: "Analysis complete.".into(),
            status: "success".into(),
            elapsed_ms: 1234,
            team_outputs: None,
        };
        assert_eq!(roundtrip(&msg), msg);

        let msg_with_team = AgentServiceResponse {
            output: "Team done.".into(),
            status: "success".into(),
            elapsed_ms: 5000,
            team_outputs: Some(vec![serde_json::json!({"agent": "a1", "output": "ok"})]),
        };
        assert_eq!(roundtrip(&msg_with_team), msg_with_team);
    }

    #[test]
    fn roundtrip_athena_request_variants() {
        let variants: Vec<AthenaServiceRequest> = vec![
            AthenaServiceRequest::Query {
                connection_id: "conn-1".into(),
                sql: "SELECT * FROM logs".into(),
                database: Some("analytics".into()),
            },
            AthenaServiceRequest::QueryParquet {
                connection_id: "conn-1".into(),
                sql: "SELECT * FROM events".into(),
                database: None,
            },
            AthenaServiceRequest::SchemaRefresh {
                connection_id: "conn-1".into(),
            },
        ];
        for v in &variants {
            assert_eq!(roundtrip(v), *v);
        }
    }

    #[test]
    fn roundtrip_athena_response_variants() {
        let variants: Vec<AthenaServiceResponse> = vec![
            AthenaServiceResponse::Status {
                state: "RUNNING".into(),
                stats: Some(serde_json::json!({"scanned_bytes": 1024})),
            },
            AthenaServiceResponse::Columns {
                columns: vec![
                    AthenaColumnInfo {
                        name: "id".into(),
                        data_type: "bigint".into(),
                    },
                    AthenaColumnInfo {
                        name: "name".into(),
                        data_type: "varchar".into(),
                    },
                ],
            },
            AthenaServiceResponse::Rows {
                rows: vec![vec![serde_json::json!(1), serde_json::json!("alice")]],
            },
            AthenaServiceResponse::Parquet {
                data: vec![0x50, 0x41, 0x52, 0x31],
            },
            AthenaServiceResponse::SchemaRefreshed {
                status: "ok".into(),
            },
            AthenaServiceResponse::Done {
                total_rows: Some(42),
            },
            AthenaServiceResponse::Error {
                message: "table not found".into(),
            },
        ];
        for v in &variants {
            assert_eq!(roundtrip(v), *v);
        }
    }

    #[test]
    fn roundtrip_catalog_request() {
        let msg = CatalogQueryRequest {
            steps: vec![
                serde_json::json!({"op": "scan", "table": "entities"}),
                serde_json::json!({"op": "filter", "field": "type", "value": "user"}),
            ],
        };
        assert_eq!(roundtrip(&msg), msg);
    }

    #[test]
    fn roundtrip_catalog_response() {
        let msg = CatalogQueryResponse {
            results: vec![serde_json::json!({"id": "e-1", "type": "user"})],
        };
        assert_eq!(roundtrip(&msg), msg);
    }

    #[test]
    fn roundtrip_service_error() {
        let msg = ServiceError {
            code: 500,
            message: "internal server error".into(),
        };
        assert_eq!(roundtrip(&msg), msg);
    }

    #[test]
    fn agent_response_omits_none_team_outputs() {
        let msg = AgentServiceResponse {
            output: "done".into(),
            status: "success".into(),
            elapsed_ms: 100,
            team_outputs: None,
        };
        // Verify JSON serialization omits the field (MessagePack includes it,
        // but JSON skip_serializing_if confirms the serde attribute works).
        let json = serde_json::to_value(&msg).unwrap();
        assert!(!json.as_object().unwrap().contains_key("team_outputs"));
    }

    #[test]
    fn roundtrip_via_message_envelope() {
        use crate::Message;

        let req = QueryServiceRequest {
            question: "Show top entities".into(),
        };
        let msg = Message::new("eisenbahn.svc.query.request", &req).unwrap();
        let decoded: QueryServiceRequest = msg.decode().unwrap();
        assert_eq!(decoded, req);
    }
}
