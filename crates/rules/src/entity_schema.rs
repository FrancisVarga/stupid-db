//! EntitySchema rule kind — defines entity types, field mappings, edges,
//! and embedding templates externalized from hardcoded Rust dispatch tables.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::schema::CommonMetadata;

// ── YAML-level types ────────────────────────────────────────────────

/// Top-level EntitySchema rule document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EntitySchemaRule {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: CommonMetadata,
    pub spec: EntitySchemaSpec,
}

/// Specification section of an EntitySchema rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EntitySchemaSpec {
    /// Values treated as null/missing during extraction.
    #[serde(default)]
    pub null_values: Vec<String>,
    /// All entity types with their key prefixes.
    pub entity_types: Vec<EntityTypeDef>,
    /// All edge types with source/target entity types.
    pub edge_types: Vec<EdgeTypeDef>,
    /// Field-to-entity mappings with aliases for cross-crate consistency.
    pub field_mappings: Vec<FieldMapping>,
    /// Per-event-type extraction plans.
    #[serde(default)]
    pub event_extraction: HashMap<String, EventExtractionPlan>,
    /// Per-event-type embedding template format strings.
    #[serde(default)]
    pub embedding_templates: HashMap<String, String>,
}

/// Definition of an entity type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EntityTypeDef {
    /// Name matching the Rust `EntityType` enum variant.
    pub name: String,
    /// Key prefix for graph node keys (e.g., "member:", "device:").
    pub key_prefix: String,
}

/// Definition of an edge type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EdgeTypeDef {
    /// Name matching the Rust `EdgeType` enum variant.
    pub name: String,
    /// Source entity type name.
    pub from: String,
    /// Target entity type name.
    pub to: String,
}

/// Mapping from a document field name to an entity type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FieldMapping {
    /// Canonical field name used in code.
    pub field: String,
    /// Alternative field names that resolve to the same entity.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Entity type this field maps to.
    pub entity_type: String,
    /// Key prefix for the entity derived from this field.
    pub key_prefix: String,
}

/// Extraction plan for a specific event type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EventExtractionPlan {
    /// Event type aliases (e.g., "PopUpModule" is an alias for "PopupModule").
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Entities to extract from this event type.
    pub entities: Vec<EntityExtraction>,
    /// Edges to create between extracted entities.
    #[serde(default)]
    pub edges: Vec<EdgeExtraction>,
}

/// A single entity extraction directive.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EntityExtraction {
    /// Field name to read from the document.
    pub field: String,
    /// Entity type to create.
    pub entity_type: String,
    /// Optional fallback fields (tried in order if primary is missing).
    #[serde(default)]
    pub fallback_fields: Vec<String>,
}

/// An edge extraction directive between two extracted entities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EdgeExtraction {
    /// Field containing the source entity.
    pub from_field: String,
    /// Field containing the target entity.
    pub to_field: String,
    /// Edge type to create.
    pub edge: String,
}

// ── Compiled (hot-path) types ───────────────────────────────────────

/// Pre-compiled entity schema with O(1) lookups for hot-path performance.
///
/// Built once from `EntitySchemaRule` at load/reload time, shared via `Arc`.
#[derive(Debug, Clone)]
pub struct CompiledEntitySchema {
    /// Field name (including aliases) → entity type name.
    pub field_to_entity: HashMap<String, String>,
    /// Entity type name → key prefix.
    pub key_prefixes: HashMap<String, String>,
    /// Event type (including aliases) → compiled extraction plan.
    pub event_extractors: HashMap<String, CompiledEventExtractor>,
    /// Values treated as null/missing.
    pub null_values: std::collections::HashSet<String>,
    /// Event type → pre-parsed template segments.
    pub embedding_templates: HashMap<String, Vec<TemplateSegment>>,
}

/// A compiled extraction plan for a single event type.
#[derive(Debug, Clone)]
pub struct CompiledEventExtractor {
    pub entities: Vec<EntityExtraction>,
    pub edges: Vec<EdgeExtraction>,
}

/// A segment of a parsed embedding template.
#[derive(Debug, Clone, PartialEq)]
pub enum TemplateSegment {
    /// Literal text to emit as-is.
    Literal(String),
    /// A field placeholder: emit the field's value.
    Field(String),
    /// A field with truncation filter: emit at most N chars.
    FieldTruncated(String, usize),
}

impl EntitySchemaRule {
    /// Compile the YAML schema into optimized lookup structures.
    pub fn compile(&self) -> CompiledEntitySchema {
        let mut field_to_entity = HashMap::new();
        let mut key_prefixes = HashMap::new();

        // Build entity type → key prefix map.
        for et in &self.spec.entity_types {
            key_prefixes.insert(et.name.clone(), et.key_prefix.clone());
        }

        // Build field → entity map (with aliases flattened).
        for fm in &self.spec.field_mappings {
            field_to_entity.insert(fm.field.clone(), fm.entity_type.clone());
            for alias in &fm.aliases {
                field_to_entity.insert(alias.clone(), fm.entity_type.clone());
            }
        }

        // Build event extractors (with aliases).
        let mut event_extractors = HashMap::new();
        for (event_type, plan) in &self.spec.event_extraction {
            let compiled = CompiledEventExtractor {
                entities: plan.entities.clone(),
                edges: plan.edges.clone(),
            };
            event_extractors.insert(event_type.clone(), compiled.clone());
            for alias in &plan.aliases {
                event_extractors.insert(alias.clone(), compiled.clone());
            }
        }

        // Build null values set.
        let null_values: std::collections::HashSet<String> =
            self.spec.null_values.iter().cloned().collect();

        // Parse embedding templates.
        let embedding_templates: HashMap<String, Vec<TemplateSegment>> = self
            .spec
            .embedding_templates
            .iter()
            .map(|(k, v)| (k.clone(), parse_template(v)))
            .collect();

        CompiledEntitySchema {
            field_to_entity,
            key_prefixes,
            event_extractors,
            null_values,
            embedding_templates,
        }
    }
}

/// Parse a template string like "Login member:{memberCode} platform:{platform}"
/// into segments. Supports `{field|truncate:N}` filter syntax.
fn parse_template(template: &str) -> Vec<TemplateSegment> {
    let mut segments = Vec::new();
    let mut remaining = template;

    while let Some(start) = remaining.find('{') {
        // Emit literal text before the placeholder.
        if start > 0 {
            segments.push(TemplateSegment::Literal(remaining[..start].to_string()));
        }

        // Find the closing brace.
        if let Some(end) = remaining[start..].find('}') {
            let inner = &remaining[start + 1..start + end];
            if let Some(pipe_pos) = inner.find("|truncate:") {
                let field = inner[..pipe_pos].to_string();
                let n_str = &inner[pipe_pos + 10..];
                let n: usize = n_str.parse().unwrap_or(100);
                segments.push(TemplateSegment::FieldTruncated(field, n));
            } else {
                segments.push(TemplateSegment::Field(inner.to_string()));
            }
            remaining = &remaining[start + end + 1..];
        } else {
            // No closing brace — emit rest as literal.
            segments.push(TemplateSegment::Literal(remaining.to_string()));
            remaining = "";
        }
    }

    // Emit any trailing literal.
    if !remaining.is_empty() {
        segments.push(TemplateSegment::Literal(remaining.to_string()));
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_entity_schema_yaml() {
        let yaml = include_str!("../../../data/rules/schema/entity-schema.yml");
        let rule: EntitySchemaRule = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(rule.kind, "EntitySchema");
        assert_eq!(rule.spec.entity_types.len(), 10);
        assert_eq!(rule.spec.edge_types.len(), 9);
        assert!(!rule.spec.field_mappings.is_empty());
        assert!(!rule.spec.event_extraction.is_empty());
    }

    #[test]
    fn compile_flattens_aliases() {
        let yaml = include_str!("../../../data/rules/schema/entity-schema.yml");
        let rule: EntitySchemaRule = serde_yaml::from_str(yaml).unwrap();
        let compiled = rule.compile();

        // "fingerprint" and "deviceId" should both map to Device.
        assert_eq!(compiled.field_to_entity.get("fingerprint").map(|s| s.as_str()), Some("Device"));
        assert_eq!(compiled.field_to_entity.get("deviceId").map(|s| s.as_str()), Some("Device"));

        // "PopUpModule" should resolve to the same extractor as "PopupModule".
        assert!(compiled.event_extractors.contains_key("PopUpModule"));
    }

    #[test]
    fn compile_null_values() {
        let yaml = include_str!("../../../data/rules/schema/entity-schema.yml");
        let rule: EntitySchemaRule = serde_yaml::from_str(yaml).unwrap();
        let compiled = rule.compile();

        assert!(compiled.null_values.contains("None"));
        assert!(compiled.null_values.contains("null"));
        assert!(compiled.null_values.contains("undefined"));
    }

    #[test]
    fn template_parsing_simple() {
        let segments = parse_template("Login member:{memberCode} platform:{platform}");
        assert_eq!(segments, vec![
            TemplateSegment::Literal("Login member:".to_string()),
            TemplateSegment::Field("memberCode".to_string()),
            TemplateSegment::Literal(" platform:".to_string()),
            TemplateSegment::Field("platform".to_string()),
        ]);
    }

    #[test]
    fn template_parsing_truncate_filter() {
        let segments = parse_template("Error: {error|truncate:100}");
        assert_eq!(segments, vec![
            TemplateSegment::Literal("Error: ".to_string()),
            TemplateSegment::FieldTruncated("error".to_string(), 100),
        ]);
    }

    #[test]
    fn round_trip() {
        let yaml = include_str!("../../../data/rules/schema/entity-schema.yml");
        let rule: EntitySchemaRule = serde_yaml::from_str(yaml).unwrap();
        let serialized = serde_yaml::to_string(&rule).unwrap();
        let rule2: EntitySchemaRule = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(rule, rule2);
    }
}
