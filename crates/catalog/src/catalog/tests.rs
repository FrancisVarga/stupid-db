#[cfg(test)]
mod tests {
    use crate::catalog::types::*;
    use stupid_core::{EdgeType, EntityType};
    use stupid_graph::GraphStore;

    fn build_test_graph() -> GraphStore {
        let mut g = GraphStore::new();
        let seg = "test".to_string();

        let a = g.upsert_node(EntityType::Member, "alice", &seg);
        let b = g.upsert_node(EntityType::Member, "bob", &seg);
        let d = g.upsert_node(EntityType::Device, "iphone-1", &seg);

        g.add_edge(a, d, EdgeType::LoggedInFrom, &seg);
        g.add_edge(b, d, EdgeType::LoggedInFrom, &seg);
        g
    }

    #[test]
    fn catalog_from_graph() {
        let g = build_test_graph();
        let cat = Catalog::from_graph(&g);

        assert_eq!(cat.total_nodes, 3);
        assert_eq!(cat.total_edges, 2);
        assert_eq!(cat.entity_types.len(), 2);
        assert_eq!(cat.edge_types.len(), 1);

        // Members should come first (2 > 1)
        assert_eq!(cat.entity_types[0].entity_type, "Member");
        assert_eq!(cat.entity_types[0].node_count, 2);

        // Edge should be LoggedInFrom: Member -> Device
        assert_eq!(cat.edge_types[0].edge_type, "LoggedInFrom");
        assert_eq!(cat.edge_types[0].source_types, vec!["Member"]);
        assert_eq!(cat.edge_types[0].target_types, vec!["Device"]);
    }

    #[test]
    fn catalog_system_prompt() {
        let g = build_test_graph();
        let cat = Catalog::from_graph(&g);
        let prompt = cat.to_system_prompt();

        assert!(prompt.contains("3 nodes"));
        assert!(prompt.contains("2 edges"));
        assert!(prompt.contains("Member"));
        assert!(prompt.contains("LoggedInFrom"));
        assert!(prompt.contains("Member \u{2192} Device"));
    }

    #[test]
    fn catalog_empty_graph() {
        let g = GraphStore::new();
        let cat = Catalog::from_graph(&g);
        assert_eq!(cat.total_nodes, 0);
        assert_eq!(cat.total_edges, 0);
        assert!(cat.entity_types.is_empty());
        assert!(cat.edge_types.is_empty());
        assert!(cat.external_sources.is_empty());
    }

    #[test]
    fn catalog_with_external_sources() {
        let g = GraphStore::new();
        let cat = Catalog::from_graph(&g).with_external_sources(vec![ExternalSource {
            name: "Data Lake".to_string(),
            kind: "athena".to_string(),
            connection_id: "prod-lake".to_string(),
            databases: vec![ExternalDatabase {
                name: "analytics".to_string(),
                tables: vec![ExternalTable {
                    name: "events".to_string(),
                    columns: vec![
                        ExternalColumn {
                            name: "id".to_string(),
                            data_type: "bigint".to_string(),
                        },
                        ExternalColumn {
                            name: "ts".to_string(),
                            data_type: "timestamp".to_string(),
                        },
                    ],
                }],
            }],
        }]);

        assert_eq!(cat.external_sources.len(), 1);
        assert_eq!(cat.external_sources[0].name, "Data Lake");
        assert_eq!(
            cat.external_sources[0].databases[0].tables[0].columns.len(),
            2
        );

        let prompt = cat.to_system_prompt();
        assert!(prompt.contains("External SQL sources:"));
        assert!(prompt.contains("Data Lake"));
        assert!(prompt.contains("athena"));
        assert!(prompt.contains("database analytics:"));
        assert!(prompt.contains("table events"));
        assert!(prompt.contains("id bigint"));
        assert!(prompt.contains("ts timestamp"));
    }

    #[test]
    fn catalog_prompt_omits_empty_external() {
        let g = build_test_graph();
        let cat = Catalog::from_graph(&g);
        let prompt = cat.to_system_prompt();
        assert!(!prompt.contains("External SQL sources:"));
    }

    #[test]
    fn catalog_json_round_trip() {
        let g = build_test_graph();
        let cat = Catalog::from_graph(&g).with_external_sources(vec![ExternalSource {
            name: "Lake".to_string(),
            kind: "athena".to_string(),
            connection_id: "prod".to_string(),
            databases: vec![ExternalDatabase {
                name: "db1".to_string(),
                tables: vec![ExternalTable {
                    name: "t1".to_string(),
                    columns: vec![ExternalColumn {
                        name: "id".to_string(),
                        data_type: "bigint".to_string(),
                    }],
                }],
            }],
        }]);

        let json = serde_json::to_string(&cat).expect("serialize");
        let restored: Catalog = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.total_nodes, cat.total_nodes);
        assert_eq!(restored.total_edges, cat.total_edges);
        assert_eq!(restored.entity_types.len(), cat.entity_types.len());
        assert_eq!(restored.edge_types.len(), cat.edge_types.len());
        assert_eq!(restored.entity_types[0].entity_type, "Member");
        assert_eq!(restored.edge_types[0].edge_type, "LoggedInFrom");
        assert_eq!(restored.external_sources.len(), 1);
        assert_eq!(restored.external_sources[0].name, "Lake");
        assert_eq!(
            restored.external_sources[0].databases[0].tables[0].columns[0].name,
            "id"
        );
    }

    #[test]
    fn catalog_json_round_trip_no_external() {
        let g = build_test_graph();
        let cat = Catalog::from_graph(&g);

        let json = serde_json::to_string(&cat).expect("serialize");
        // external_sources omitted due to skip_serializing_if
        assert!(!json.contains("external_sources"));

        let restored: Catalog = serde_json::from_str(&json).expect("deserialize");
        assert!(restored.external_sources.is_empty());
        assert_eq!(restored.total_nodes, 3);
    }

    // -- PartialCatalog tests --

    fn build_two_segment_graph() -> GraphStore {
        let mut g = GraphStore::new();

        let a = g.upsert_node(EntityType::Member, "alice", &"seg-a".to_string());
        let d = g.upsert_node(EntityType::Device, "iphone-1", &"seg-a".to_string());
        g.add_edge(a, d, EdgeType::LoggedInFrom, &"seg-a".to_string());

        let b = g.upsert_node(EntityType::Member, "bob", &"seg-b".to_string());
        let d2 = g.upsert_node(EntityType::Device, "android-1", &"seg-b".to_string());
        g.add_edge(b, d2, EdgeType::LoggedInFrom, &"seg-b".to_string());

        // Bob also appears in seg-a (shared node)
        g.upsert_node(EntityType::Member, "bob", &"seg-a".to_string());

        g
    }

    #[test]
    fn partial_catalog_from_segment() {
        let g = build_two_segment_graph();
        let partial_a = PartialCatalog::from_graph_segment(&g, "seg-a");

        assert_eq!(partial_a.segment_id, "seg-a");
        // seg-a has: alice, bob (shared), iphone-1 = 3 nodes
        assert_eq!(partial_a.node_count, 3);
        // seg-a has 1 edge (alice -> iphone-1)
        assert_eq!(partial_a.edge_count, 1);
        assert_eq!(partial_a.entity_types.len(), 2); // Member, Device
    }

    #[test]
    fn partial_catalog_serialization() {
        let g = build_two_segment_graph();
        let partial = PartialCatalog::from_graph_segment(&g, "seg-a");

        let json = serde_json::to_string(&partial).expect("serialize");
        let restored: PartialCatalog = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.segment_id, "seg-a");
        assert_eq!(restored.node_count, partial.node_count);
        assert_eq!(restored.edge_count, partial.edge_count);
    }

    #[test]
    fn catalog_from_partials_merges_counts() {
        let g = build_two_segment_graph();
        let partial_a = PartialCatalog::from_graph_segment(&g, "seg-a");
        let partial_b = PartialCatalog::from_graph_segment(&g, "seg-b");

        let merged = Catalog::from_partials(&[partial_a.clone(), partial_b.clone()]);

        // Total counts are summed from partials
        assert_eq!(
            merged.total_nodes,
            partial_a.node_count + partial_b.node_count
        );
        assert_eq!(
            merged.total_edges,
            partial_a.edge_count + partial_b.edge_count
        );

        // Should have 2 entity types: Member and Device
        assert_eq!(merged.entity_types.len(), 2);
        // Should have 1 edge type: LoggedInFrom
        assert_eq!(merged.edge_types.len(), 1);

        // Member count = members in seg-a + members in seg-b
        let member_entry = merged
            .entity_types
            .iter()
            .find(|e| e.entity_type == "Member")
            .unwrap();
        let member_a = partial_a
            .entity_types
            .iter()
            .find(|e| e.entity_type == "Member")
            .unwrap();
        let member_b = partial_b
            .entity_types
            .iter()
            .find(|e| e.entity_type == "Member")
            .unwrap();
        assert_eq!(
            member_entry.node_count,
            member_a.node_count + member_b.node_count
        );
    }

    #[test]
    fn catalog_from_partials_empty() {
        let merged = Catalog::from_partials(&[]);
        assert_eq!(merged.total_nodes, 0);
        assert_eq!(merged.total_edges, 0);
        assert!(merged.entity_types.is_empty());
        assert!(merged.edge_types.is_empty());
    }

    #[test]
    fn catalog_from_partials_samples_capped() {
        // Build a graph with many nodes of same type to test sample cap
        let mut g = GraphStore::new();
        for i in 0..10 {
            g.upsert_node(
                EntityType::Member,
                &format!("user-{}", i),
                &"seg-x".to_string(),
            );
        }

        let partial = PartialCatalog::from_graph_segment(&g, "seg-x");
        assert_eq!(partial.entity_types[0].sample_keys.len(), 5); // capped at 5

        let merged = Catalog::from_partials(&[partial]);
        assert!(merged.entity_types[0].sample_keys.len() <= 5);
    }
}
