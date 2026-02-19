//! YAML import/export handler endpoints for Stille Post configuration.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use sqlx::types::Uuid;
use tracing::info;

use crate::state::AppState;

use super::agents::SpAgent;
use super::common::{bad_request, internal_error, require_pg, ApiResult};
use super::data_sources::SpDataSource;
use super::deliveries::SpDelivery;
use super::pipelines::{SpPipeline, SpPipelineStep};
use super::schedules::SpSchedule;
use super::yaml_types::{
    SpAgentSpec, SpDataSourceSpec, SpDeliverySpec, SpImportRequest, SpImportResult,
    SpImportedResource, SpPipelineSpec, SpPipelineStepSpec, SpScheduleSpec, SpYamlEnvelope,
    SpYamlKind, SpYamlMetadata,
};

use super::super::QueryErrorResponse;

// ── Export endpoint ──────────────────────────────────────────────

/// GET /sp/export -- export all SP configuration as multi-document YAML.
pub async fn sp_export(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let pool = require_pg(&state)?;
    let mut docs: Vec<String> = Vec::new();

    // 1. Agents
    let agents = sqlx::query_as::<_, SpAgent>(
        "SELECT id, name, description, system_prompt, model,
                skills_config, mcp_servers_config, tools_config,
                template_id, created_at, updated_at
         FROM sp_agents ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    for a in &agents {
        let skills: Vec<serde_json::Value> =
            serde_json::from_value(a.skills_config.clone()).unwrap_or_default();
        let mcp: Vec<serde_json::Value> =
            serde_json::from_value(a.mcp_servers_config.clone()).unwrap_or_default();
        let tools: Vec<serde_json::Value> =
            serde_json::from_value(a.tools_config.clone()).unwrap_or_default();

        let doc = SpYamlEnvelope {
            api_version: "v1".into(),
            kind: SpYamlKind::SpAgent,
            metadata: SpYamlMetadata {
                name: a.name.clone(),
                description: a.description.clone(),
                tags: vec![],
            },
            spec: serde_yaml::to_value(&SpAgentSpec {
                model: Some(a.model.clone()),
                system_prompt: a.system_prompt.clone(),
                template_id: a.template_id.clone(),
                skills_config: skills,
                mcp_servers_config: mcp,
                tools_config: tools,
            })
            .map_err(|e| internal_error(e))?,
        };
        docs.push(serde_yaml::to_string(&doc).map_err(|e| internal_error(e))?);
    }

    // 2. Data sources
    let sources = sqlx::query_as::<_, SpDataSource>(
        "SELECT id, name, source_type, config_json, created_at, updated_at
         FROM sp_data_sources ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    for ds in &sources {
        let doc = SpYamlEnvelope {
            api_version: "v1".into(),
            kind: SpYamlKind::SpDataSource,
            metadata: SpYamlMetadata {
                name: ds.name.clone(),
                description: None,
                tags: vec![],
            },
            spec: serde_yaml::to_value(&SpDataSourceSpec {
                source_type: ds.source_type.clone(),
                config: ds.config_json.clone(),
            })
            .map_err(|e| internal_error(e))?,
        };
        docs.push(serde_yaml::to_string(&doc).map_err(|e| internal_error(e))?);
    }

    // 3. Pipelines (with steps, agents/sources resolved to names)
    let agent_map: HashMap<Uuid, String> = agents.iter().map(|a| (a.id, a.name.clone())).collect();
    let source_map: HashMap<Uuid, String> =
        sources.iter().map(|s| (s.id, s.name.clone())).collect();

    let pipelines = sqlx::query_as::<_, SpPipeline>(
        "SELECT id, name, description, created_at, updated_at FROM sp_pipelines ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    for p in &pipelines {
        let steps = sqlx::query_as::<_, SpPipelineStep>(
            "SELECT id, pipeline_id, agent_id, step_order, input_mapping,
                    output_mapping, parallel_group, data_source_id
             FROM sp_pipeline_steps WHERE pipeline_id = $1 ORDER BY step_order",
        )
        .bind(p.id)
        .fetch_all(pool)
        .await
        .map_err(internal_error)?;

        let step_specs: Vec<SpPipelineStepSpec> = steps
            .iter()
            .map(|s| SpPipelineStepSpec {
                step_order: s.step_order,
                agent_name: s.agent_id.and_then(|id| agent_map.get(&id).cloned()),
                data_source_name: s
                    .data_source_id
                    .and_then(|id| source_map.get(&id).cloned()),
                parallel_group: s.parallel_group,
                input_mapping: s.input_mapping.clone(),
                output_mapping: s.output_mapping.clone(),
            })
            .collect();

        let doc = SpYamlEnvelope {
            api_version: "v1".into(),
            kind: SpYamlKind::SpPipeline,
            metadata: SpYamlMetadata {
                name: p.name.clone(),
                description: p.description.clone(),
                tags: vec![],
            },
            spec: serde_yaml::to_value(&SpPipelineSpec { steps: step_specs })
                .map_err(|e| internal_error(e))?,
        };
        docs.push(serde_yaml::to_string(&doc).map_err(|e| internal_error(e))?);
    }

    // 4. Schedules (pipeline resolved to name)
    let pipeline_map: HashMap<Uuid, String> =
        pipelines.iter().map(|p| (p.id, p.name.clone())).collect();

    let schedules = sqlx::query_as::<_, SpSchedule>(
        "SELECT id, pipeline_id, cron_expression, timezone, enabled,
                last_run_at, next_run_at, created_at
         FROM sp_schedules ORDER BY created_at",
    )
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    let mut schedule_name_map: HashMap<Uuid, String> = HashMap::new();

    for sch in &schedules {
        let sched_name = format!(
            "{}-schedule",
            pipeline_map
                .get(&sch.pipeline_id)
                .cloned()
                .unwrap_or_else(|| sch.id.to_string())
        );
        schedule_name_map.insert(sch.id, sched_name.clone());

        let doc = SpYamlEnvelope {
            api_version: "v1".into(),
            kind: SpYamlKind::SpSchedule,
            metadata: SpYamlMetadata {
                name: sched_name,
                description: None,
                tags: vec![],
            },
            spec: serde_yaml::to_value(&SpScheduleSpec {
                pipeline_name: pipeline_map
                    .get(&sch.pipeline_id)
                    .cloned()
                    .unwrap_or_else(|| sch.pipeline_id.to_string()),
                cron_expression: sch.cron_expression.clone(),
                timezone: sch.timezone.clone(),
                enabled: sch.enabled,
            })
            .map_err(|e| internal_error(e))?,
        };
        docs.push(serde_yaml::to_string(&doc).map_err(|e| internal_error(e))?);
    }

    // 5. Deliveries (schedule resolved to name)
    let deliveries = sqlx::query_as::<_, SpDelivery>(
        "SELECT id, schedule_id, channel, config_json, enabled FROM sp_deliveries ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    for d in &deliveries {
        let sched_name = d
            .schedule_id
            .and_then(|sid| schedule_name_map.get(&sid).cloned())
            .unwrap_or_else(|| "unknown".into());

        let doc = SpYamlEnvelope {
            api_version: "v1".into(),
            kind: SpYamlKind::SpDelivery,
            metadata: SpYamlMetadata {
                name: format!("{}-{}", sched_name, d.channel),
                description: None,
                tags: vec![],
            },
            spec: serde_yaml::to_value(&SpDeliverySpec {
                schedule_name: sched_name,
                channel: d.channel.clone(),
                enabled: d.enabled,
                config: d.config_json.clone(),
            })
            .map_err(|e| internal_error(e))?,
        };
        docs.push(serde_yaml::to_string(&doc).map_err(|e| internal_error(e))?);
    }

    // Join with YAML document separators
    let yaml_output = docs.join("---\n");

    Ok((
        [(header::CONTENT_TYPE, "application/x-yaml")],
        yaml_output,
    ))
}

// ── Import endpoint ──────────────────────────────────────────────

/// POST /sp/import -- import SP configuration from multi-document YAML.
///
/// Resources are created in dependency order:
/// agents -> data sources -> pipelines -> schedules -> deliveries.
/// Names are used as keys for cross-references (not UUIDs).
pub async fn sp_import(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SpImportRequest>,
) -> ApiResult<Json<SpImportResult>> {
    let pool = require_pg(&state)?;

    // Parse multi-document YAML
    let mut envelopes: Vec<SpYamlEnvelope> = Vec::new();
    for doc_str in req.yaml.split("\n---") {
        let trimmed = doc_str.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') && !trimmed.contains("apiVersion") {
            continue;
        }
        match serde_yaml::from_str::<SpYamlEnvelope>(trimmed) {
            Ok(env) => envelopes.push(env),
            Err(e) => {
                // Skip unparseable fragments (e.g. comment-only blocks)
                info!("Skipping YAML fragment: {}", e);
            }
        }
    }

    let mut result = SpImportResult {
        created: vec![],
        updated: vec![],
        skipped: vec![],
        errors: vec![],
    };

    // Sort by dependency order
    let order = |k: &SpYamlKind| match k {
        SpYamlKind::SpAgent => 0,
        SpYamlKind::SpDataSource => 1,
        SpYamlKind::SpPipeline => 2,
        SpYamlKind::SpSchedule => 3,
        SpYamlKind::SpDelivery => 4,
    };
    envelopes.sort_by_key(|e| order(&e.kind));

    // Name -> UUID maps for cross-reference resolution
    let mut agent_ids: HashMap<String, Uuid> = HashMap::new();
    let mut source_ids: HashMap<String, Uuid> = HashMap::new();
    let mut pipeline_ids: HashMap<String, Uuid> = HashMap::new();
    let mut schedule_ids: HashMap<String, Uuid> = HashMap::new();

    // Pre-populate maps from existing DB rows
    let existing_agents: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM sp_agents")
            .fetch_all(pool)
            .await
            .map_err(internal_error)?;
    for (id, name) in &existing_agents {
        agent_ids.insert(name.clone(), *id);
    }

    let existing_sources: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM sp_data_sources")
            .fetch_all(pool)
            .await
            .map_err(internal_error)?;
    for (id, name) in &existing_sources {
        source_ids.insert(name.clone(), *id);
    }

    let existing_pipelines: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM sp_pipelines")
            .fetch_all(pool)
            .await
            .map_err(internal_error)?;
    for (id, name) in &existing_pipelines {
        pipeline_ids.insert(name.clone(), *id);
    }

    // Note: schedules don't have names in the DB, so we track by
    // our import-generated names in schedule_ids.

    for envelope in &envelopes {
        let name = &envelope.metadata.name;
        match envelope.kind {
            SpYamlKind::SpAgent => {
                let spec: SpAgentSpec = serde_yaml::from_value(envelope.spec.clone())
                    .map_err(|e| bad_request(format!("Invalid SpAgent spec '{}': {}", name, e)))?;

                if let Some(existing_id) = agent_ids.get(name) {
                    if req.overwrite {
                        sqlx::query(
                            "UPDATE sp_agents SET description=$1, system_prompt=$2, model=$3,
                             skills_config=$4, mcp_servers_config=$5, tools_config=$6,
                             template_id=$7, updated_at=now() WHERE id=$8",
                        )
                        .bind(&envelope.metadata.description)
                        .bind(&spec.system_prompt)
                        .bind(spec.model.as_deref().unwrap_or("claude-sonnet-4-6"))
                        .bind(serde_json::to_value(&spec.skills_config).unwrap_or_default())
                        .bind(
                            serde_json::to_value(&spec.mcp_servers_config).unwrap_or_default(),
                        )
                        .bind(serde_json::to_value(&spec.tools_config).unwrap_or_default())
                        .bind(&spec.template_id)
                        .bind(existing_id)
                        .execute(pool)
                        .await
                        .map_err(internal_error)?;
                        result.updated.push(SpImportedResource {
                            kind: "SpAgent".into(),
                            name: name.clone(),
                        });
                    } else {
                        result.skipped.push(SpImportedResource {
                            kind: "SpAgent".into(),
                            name: name.clone(),
                        });
                    }
                } else {
                    let row: (Uuid,) = sqlx::query_as(
                        "INSERT INTO sp_agents (name, description, system_prompt, model,
                         skills_config, mcp_servers_config, tools_config, template_id)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
                    )
                    .bind(name)
                    .bind(&envelope.metadata.description)
                    .bind(&spec.system_prompt)
                    .bind(spec.model.as_deref().unwrap_or("claude-sonnet-4-6"))
                    .bind(serde_json::to_value(&spec.skills_config).unwrap_or_default())
                    .bind(
                        serde_json::to_value(&spec.mcp_servers_config).unwrap_or_default(),
                    )
                    .bind(serde_json::to_value(&spec.tools_config).unwrap_or_default())
                    .bind(&spec.template_id)
                    .fetch_one(pool)
                    .await
                    .map_err(internal_error)?;
                    agent_ids.insert(name.clone(), row.0);
                    result.created.push(SpImportedResource {
                        kind: "SpAgent".into(),
                        name: name.clone(),
                    });
                }
            }

            SpYamlKind::SpDataSource => {
                let spec: SpDataSourceSpec =
                    serde_yaml::from_value(envelope.spec.clone()).map_err(|e| {
                        bad_request(format!("Invalid SpDataSource spec '{}': {}", name, e))
                    })?;

                if let Some(existing_id) = source_ids.get(name) {
                    if req.overwrite {
                        sqlx::query(
                            "UPDATE sp_data_sources SET source_type=$1, config_json=$2,
                             updated_at=now() WHERE id=$3",
                        )
                        .bind(&spec.source_type)
                        .bind(&spec.config)
                        .bind(existing_id)
                        .execute(pool)
                        .await
                        .map_err(internal_error)?;
                        result.updated.push(SpImportedResource {
                            kind: "SpDataSource".into(),
                            name: name.clone(),
                        });
                    } else {
                        result.skipped.push(SpImportedResource {
                            kind: "SpDataSource".into(),
                            name: name.clone(),
                        });
                    }
                } else {
                    let row: (Uuid,) = sqlx::query_as(
                        "INSERT INTO sp_data_sources (name, source_type, config_json)
                         VALUES ($1,$2,$3) RETURNING id",
                    )
                    .bind(name)
                    .bind(&spec.source_type)
                    .bind(&spec.config)
                    .fetch_one(pool)
                    .await
                    .map_err(internal_error)?;
                    source_ids.insert(name.clone(), row.0);
                    result.created.push(SpImportedResource {
                        kind: "SpDataSource".into(),
                        name: name.clone(),
                    });
                }
            }

            SpYamlKind::SpPipeline => {
                let spec: SpPipelineSpec =
                    serde_yaml::from_value(envelope.spec.clone()).map_err(|e| {
                        bad_request(format!("Invalid SpPipeline spec '{}': {}", name, e))
                    })?;

                let pipeline_id = if let Some(existing_id) = pipeline_ids.get(name) {
                    if req.overwrite {
                        sqlx::query(
                            "UPDATE sp_pipelines SET description=$1, updated_at=now() WHERE id=$2",
                        )
                        .bind(&envelope.metadata.description)
                        .bind(existing_id)
                        .execute(pool)
                        .await
                        .map_err(internal_error)?;
                        // Delete old steps before re-creating
                        sqlx::query("DELETE FROM sp_pipeline_steps WHERE pipeline_id = $1")
                            .bind(existing_id)
                            .execute(pool)
                            .await
                            .map_err(internal_error)?;
                        result.updated.push(SpImportedResource {
                            kind: "SpPipeline".into(),
                            name: name.clone(),
                        });
                        *existing_id
                    } else {
                        result.skipped.push(SpImportedResource {
                            kind: "SpPipeline".into(),
                            name: name.clone(),
                        });
                        continue;
                    }
                } else {
                    let row: (Uuid,) = sqlx::query_as(
                        "INSERT INTO sp_pipelines (name, description) VALUES ($1,$2) RETURNING id",
                    )
                    .bind(name)
                    .bind(&envelope.metadata.description)
                    .fetch_one(pool)
                    .await
                    .map_err(internal_error)?;
                    pipeline_ids.insert(name.clone(), row.0);
                    result.created.push(SpImportedResource {
                        kind: "SpPipeline".into(),
                        name: name.clone(),
                    });
                    row.0
                };

                // Insert steps with name -> UUID resolution
                for step in &spec.steps {
                    let agent_id = step
                        .agent_name
                        .as_ref()
                        .and_then(|n| agent_ids.get(n).copied());
                    let ds_id = step
                        .data_source_name
                        .as_ref()
                        .and_then(|n| source_ids.get(n).copied());

                    sqlx::query(
                        "INSERT INTO sp_pipeline_steps
                         (pipeline_id, agent_id, step_order, input_mapping,
                          output_mapping, parallel_group, data_source_id)
                         VALUES ($1,$2,$3,$4,$5,$6,$7)",
                    )
                    .bind(pipeline_id)
                    .bind(agent_id)
                    .bind(step.step_order)
                    .bind(&step.input_mapping)
                    .bind(&step.output_mapping)
                    .bind(step.parallel_group)
                    .bind(ds_id)
                    .execute(pool)
                    .await
                    .map_err(internal_error)?;
                }
            }

            SpYamlKind::SpSchedule => {
                let spec: SpScheduleSpec =
                    serde_yaml::from_value(envelope.spec.clone()).map_err(|e| {
                        bad_request(format!("Invalid SpSchedule spec '{}': {}", name, e))
                    })?;

                let pip_id =
                    pipeline_ids
                        .get(&spec.pipeline_name)
                        .copied()
                        .ok_or_else(|| {
                            bad_request(format!(
                                "Schedule '{}' references unknown pipeline '{}'",
                                name, spec.pipeline_name
                            ))
                        })?;

                let row: (Uuid,) = sqlx::query_as(
                    "INSERT INTO sp_schedules (pipeline_id, cron_expression, timezone, enabled)
                     VALUES ($1,$2,$3,$4) RETURNING id",
                )
                .bind(pip_id)
                .bind(&spec.cron_expression)
                .bind(&spec.timezone)
                .bind(spec.enabled)
                .fetch_one(pool)
                .await
                .map_err(internal_error)?;
                schedule_ids.insert(name.clone(), row.0);
                result.created.push(SpImportedResource {
                    kind: "SpSchedule".into(),
                    name: name.clone(),
                });
            }

            SpYamlKind::SpDelivery => {
                let spec: SpDeliverySpec =
                    serde_yaml::from_value(envelope.spec.clone()).map_err(|e| {
                        bad_request(format!("Invalid SpDelivery spec '{}': {}", name, e))
                    })?;

                let sched_id =
                    schedule_ids
                        .get(&spec.schedule_name)
                        .copied()
                        .ok_or_else(|| {
                            bad_request(format!(
                                "Delivery '{}' references unknown schedule '{}'",
                                name, spec.schedule_name
                            ))
                        })?;

                sqlx::query(
                    "INSERT INTO sp_deliveries (schedule_id, channel, config_json, enabled)
                     VALUES ($1,$2,$3,$4)",
                )
                .bind(sched_id)
                .bind(&spec.channel)
                .bind(&spec.config)
                .bind(spec.enabled)
                .execute(pool)
                .await
                .map_err(internal_error)?;
                result.created.push(SpImportedResource {
                    kind: "SpDelivery".into(),
                    name: name.clone(),
                });
            }
        }
    }

    Ok(Json(result))
}
