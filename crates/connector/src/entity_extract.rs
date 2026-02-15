use stupid_core::{Document, EdgeType, EntityType, FieldValue, SegmentId};
use stupid_graph::GraphStore;

pub struct EntityExtractor;

impl EntityExtractor {
    /// Extract entities and edges from a document, dispatching by event type.
    pub fn extract(doc: &Document, graph: &mut GraphStore, segment_id: &SegmentId) {
        match doc.event_type.as_str() {
            "Login" => Self::extract_login(doc, graph, segment_id),
            "GameOpened" => Self::extract_game_opened(doc, graph, segment_id),
            "PopupModule" | "PopUpModule" => Self::extract_popup(doc, graph, segment_id),
            "API Error" => Self::extract_api_error(doc, graph, segment_id),
            "GridClick" => Self::extract_grid_click(doc, graph, segment_id),
            _ => {}
        }
    }

    /// Login → member, device, platform, currency, vipgroup, affiliate
    fn extract_login(doc: &Document, graph: &mut GraphStore, segment_id: &SegmentId) {
        let member_id = match upsert_member(doc, graph, segment_id) {
            Some(id) => id,
            None => return,
        };

        // Device (fingerprint)
        if let Some(fp) = get_field(doc, "fingerprint") {
            let device_id = graph.upsert_node(EntityType::Device, &format!("device:{}", fp), segment_id);
            graph.add_edge(member_id, device_id, EdgeType::LoggedInFrom, segment_id);
        }

        link_platform(doc, graph, segment_id, member_id);
        link_currency(doc, graph, segment_id, member_id);
        link_vipgroup(doc, graph, segment_id, member_id);
        link_affiliate(doc, graph, segment_id, member_id);
    }

    /// GameOpened → member, game, provider, platform, currency
    fn extract_game_opened(doc: &Document, graph: &mut GraphStore, segment_id: &SegmentId) {
        let member_id = match upsert_member(doc, graph, segment_id) {
            Some(id) => id,
            None => return,
        };

        // Game (game field holds the game code/name)
        if let Some(g) = get_field(doc, "game") {
            let game_id = graph.upsert_node(EntityType::Game, &format!("game:{}", g), segment_id);
            graph.add_edge(member_id, game_id, EdgeType::OpenedGame, segment_id);

            // Provider linked to game (gameTrackingProvider)
            if let Some(p) = get_field(doc, "gameTrackingProvider") {
                let provider_id = graph.upsert_node(EntityType::Provider, &format!("provider:{}", p), segment_id);
                graph.add_edge(game_id, provider_id, EdgeType::ProvidedBy, segment_id);
            }
        }

        link_platform(doc, graph, segment_id, member_id);
        link_currency(doc, graph, segment_id, member_id);
    }

    /// PopupModule / PopUpModule → member, popup, platform
    fn extract_popup(doc: &Document, graph: &mut GraphStore, segment_id: &SegmentId) {
        let member_id = match upsert_member(doc, graph, segment_id) {
            Some(id) => id,
            None => return,
        };

        // Popup identity: prefer trackingId, fall back to popupType
        let popup_key = get_field(doc, "trackingId")
            .or_else(|| get_field(doc, "popupType"));
        if let Some(pk) = popup_key {
            let popup_id = graph.upsert_node(EntityType::Popup, &format!("popup:{}", pk), segment_id);
            graph.add_edge(member_id, popup_id, EdgeType::SawPopup, segment_id);
        }

        link_platform(doc, graph, segment_id, member_id);
    }

    /// API Error → member, error, platform
    fn extract_api_error(doc: &Document, graph: &mut GraphStore, segment_id: &SegmentId) {
        let member_id = match upsert_member(doc, graph, segment_id) {
            Some(id) => id,
            None => return,
        };

        // Error identity: combine url + statusCode for a meaningful key, fall back to error field
        let error_key = match (get_field(doc, "url"), get_field(doc, "statusCode")) {
            (Some(url), Some(code)) => Some(format!("error:{}:{}", code, url)),
            (Some(url), None) => Some(format!("error:{}", url)),
            _ => get_field(doc, "error").map(|e| format!("error:{}", e)),
        };
        if let Some(ek) = error_key {
            let error_id = graph.upsert_node(EntityType::Error, &ek, segment_id);
            graph.add_edge(member_id, error_id, EdgeType::HitError, segment_id);
        }

        link_platform(doc, graph, segment_id, member_id);
    }

    /// GridClick → member, game, platform
    fn extract_grid_click(doc: &Document, graph: &mut GraphStore, segment_id: &SegmentId) {
        let member_id = match upsert_member(doc, graph, segment_id) {
            Some(id) => id,
            None => return,
        };

        // Game (clicked in grid)
        if let Some(g) = get_field(doc, "game") {
            let game_id = graph.upsert_node(EntityType::Game, &format!("game:{}", g), segment_id);
            graph.add_edge(member_id, game_id, EdgeType::OpenedGame, segment_id);

            // Provider linked to game
            if let Some(p) = get_field(doc, "gameTrackingProvider") {
                let provider_id = graph.upsert_node(EntityType::Provider, &format!("provider:{}", p), segment_id);
                graph.add_edge(game_id, provider_id, EdgeType::ProvidedBy, segment_id);
            }
        }

        link_platform(doc, graph, segment_id, member_id);
    }
}

/// Upsert member node — returns None if memberCode is missing.
fn upsert_member(doc: &Document, graph: &mut GraphStore, segment_id: &SegmentId) -> Option<stupid_core::NodeId> {
    let code = get_field(doc, "memberCode")?;
    Some(graph.upsert_node(EntityType::Member, &format!("member:{}", code), segment_id))
}

/// Link member → platform edge.
fn link_platform(doc: &Document, graph: &mut GraphStore, segment_id: &SegmentId, member_id: stupid_core::NodeId) {
    if let Some(p) = get_field(doc, "platform") {
        let platform_id = graph.upsert_node(EntityType::Platform, &format!("platform:{}", p), segment_id);
        graph.add_edge(member_id, platform_id, EdgeType::PlaysOnPlatform, segment_id);
    }
}

/// Link member → currency edge.
fn link_currency(doc: &Document, graph: &mut GraphStore, segment_id: &SegmentId, member_id: stupid_core::NodeId) {
    if let Some(c) = get_field(doc, "currency") {
        let currency_id = graph.upsert_node(EntityType::Currency, &format!("currency:{}", c), segment_id);
        graph.add_edge(member_id, currency_id, EdgeType::UsesCurrency, segment_id);
    }
}

/// Link member → vipgroup edge.
fn link_vipgroup(doc: &Document, graph: &mut GraphStore, segment_id: &SegmentId, member_id: stupid_core::NodeId) {
    if let Some(g) = get_field(doc, "rGroup") {
        let group_id = graph.upsert_node(EntityType::VipGroup, &format!("vipgroup:{}", g), segment_id);
        graph.add_edge(member_id, group_id, EdgeType::BelongsToGroup, segment_id);
    }
}

/// Link member → affiliate edge.
fn link_affiliate(doc: &Document, graph: &mut GraphStore, segment_id: &SegmentId, member_id: stupid_core::NodeId) {
    if let Some(a) = get_affiliate(doc) {
        let affiliate_id = graph.upsert_node(EntityType::Affiliate, &format!("affiliate:{}", a), segment_id);
        graph.add_edge(member_id, affiliate_id, EdgeType::ReferredBy, segment_id);
    }
}

fn get_field<'a>(doc: &'a Document, name: &str) -> Option<&'a str> {
    doc.fields.get(name).and_then(|v| match v {
        FieldValue::Text(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() || trimmed == "None" || trimmed == "null" || trimmed == "undefined" {
                None
            } else {
                Some(trimmed)
            }
        }
        _ => None,
    })
}

fn get_affiliate(doc: &Document) -> Option<&str> {
    // Normalize the 3 different spellings
    get_field(doc, "affiliateId")
        .or_else(|| get_field(doc, "affiliateid"))
        .or_else(|| get_field(doc, "affiliateID"))
}
