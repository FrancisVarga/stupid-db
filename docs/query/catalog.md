# Knowledge Catalog

## Overview

The catalog is the system's self-awareness layer — it knows what data exists, what's been computed, and what can be queried. The LLM uses the catalog to generate valid query plans. The dashboard uses it to offer browsing and suggestions.

## Catalog Contents

### Schema Catalog

What event types exist and what fields they have:

```json
{
  "event_types": {
    "Login": {
      "document_count": 1720770,
      "field_count": 72,
      "date_range": ["2025-06-12", "2025-07-12"],
      "key_fields": [
        { "name": "memberCode", "type": "string", "null_rate": 0.0, "cardinality": 48230 },
        { "name": "success", "type": "boolean_string", "null_rate": 0.02, "values": ["true", "false"] },
        { "name": "platform", "type": "string", "null_rate": 0.0, "cardinality": 8 },
        { "name": "currency", "type": "string", "null_rate": 0.01, "values": ["VND", "THB", "IDR"] }
      ]
    },
    "GameOpened": { ... },
    "API Error": { ... },
    "PopupModule": { ... }
  }
}
```

### Entity Catalog

What entities exist in the graph:

```json
{
  "entity_types": {
    "Member": { "count": 487230, "sample": ["thongtran2904", "Soundtraz", "MQUECHUA"] },
    "Game": { "count": 1240, "sample": ["retrow88", "prosperitywheel", "baccarat"] },
    "Device": { "count": 312000, "note": "fingerprint hashes" },
    "VipGroup": { "count": 6, "values": ["VIPB", "VIPP", "VIPG", "VIPA", "VIPS", "VIPC"] },
    "Currency": { "count": 3, "values": ["VND", "THB", "IDR"] },
    "Platform": { "count": 8, "values": ["web-android", "web-ios", "android", "ios", ...] }
  },
  "edge_types": {
    "LoggedInFrom": { "count": 1720770 },
    "OpenedGame": { "count": 9231450 },
    "HitError": { "count": 12642120 },
    "SawPopup": { "count": 5151870 }
  }
}
```

### Compute Catalog

What computed results are available:

```json
{
  "clusters": {
    "count": 12,
    "algorithm": "k-means",
    "last_computed": "2025-07-12T03:00:00Z",
    "labels": [
      "High-frequency VN mobile slot players",
      "Thai card game enthusiasts",
      "Casual multi-platform browsers"
    ]
  },
  "communities": {
    "count": 47,
    "algorithm": "louvain",
    "last_computed": "2025-07-12T02:00:00Z"
  },
  "anomalies": {
    "current_count": 23,
    "threshold": 0.5
  },
  "patterns": {
    "count": 156,
    "categories": { "Churn": 34, "Engagement": 67, "ErrorChain": 28, "Funnel": 27 }
  },
  "trends": {
    "active_alerts": 2,
    "description": ["Login rate up 15% vs 7d baseline", "API Error spike in auth-service"]
  }
}
```

### Segment Catalog

What data segments are available:

```json
{
  "segments": {
    "active": "2025-07-12",
    "count": 30,
    "oldest": "2025-06-12",
    "newest": "2025-07-12",
    "total_documents": 28746000,
    "total_size_gb": 98.4
  },
  "external_segments": [
    { "uri": "s3://analytics/archive/2025-05/", "event_types": ["Login", "GameOpened"] }
  ]
}
```

## LLM System Prompt

The catalog is condensed into the LLM's system prompt so it knows what's available:

```
You are a knowledge analyst for a gaming platform database.

Available event types:
- Login (1.7M docs): memberCode, success, platform, currency, rGroup, method
- GameOpened (9.2M docs): memberCode, game, category, provider, platform
- API Error (12.6M docs): error, stage, platform, page, status
- PopupModule (5.2M docs): memberCode, popupType, clickType, componentId

Available entities: Member (487K), Game (1.2K), Device (312K), VipGroup (6), Currency (3)

Computed knowledge:
- 12 member clusters (k-means, last computed 3h ago)
- 47 communities (louvain)
- 23 active anomalies
- 156 temporal patterns (34 churn, 67 engagement, 28 error chains)
- 2 active trend alerts

You generate structured query plans. Available operations:
- document.scan: Scan events by type, time, field filters
- vector.search: Semantic similarity search
- graph.traverse: Follow edges from entities
- compute.read: Read clusters, communities, anomalies, patterns
- compute.aggregate: Group by, count, sum, avg
- compute.filter: Filter + join result sets
- compute.timeline: Bucket by time
```

## Catalog Refresh

The catalog is refreshed:
- Schema catalog: on new event type or field discovery
- Entity catalog: every 5 minutes (counts + samples)
- Compute catalog: after each compute cycle
- Segment catalog: on segment rotation / eviction

## Browse API

The dashboard can browse the catalog:

```
GET /api/catalog                    → Full catalog summary
GET /api/catalog/events             → Event type list + schemas
GET /api/catalog/events/Login       → Login event schema + stats
GET /api/catalog/entities           → Entity type list + counts
GET /api/catalog/entities/Member    → Member entity samples + stats
GET /api/catalog/compute            → Computed knowledge summary
GET /api/catalog/compute/clusters   → Cluster details
GET /api/catalog/segments           → Segment list + health
```
