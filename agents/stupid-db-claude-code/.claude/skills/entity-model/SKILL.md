---
name: entity-model
description: Entity types, edge types, event mappings, and feature vector definitions used in the knowledge graph
triggers:
  - entity
  - edge type
  - entity extraction
  - feature vector
  - graph model
  - knowledge graph
---

# Entity Model

## Entity Types (10)

Defined in `data/rules/schema/entity-schema.yml`:

| Entity | Source Field | Description |
|--------|------------|-------------|
| Member | member_code | User/player account |
| Device | device_id | Device fingerprint |
| Game | game_code | Game identifier |
| Affiliate | affiliate_code | Marketing affiliate |
| Currency | currency | Currency code |
| VipGroup | vip_group | VIP tier level |
| Error | error_code | Error classification |
| Platform | platform | Platform (mobile/desktop/etc) |
| Popup | popup_name | UI popup/modal |
| Provider | provider_code | Game/service provider |

## Edge Types (9)

| Edge | From | To | Derived From |
|------|------|-----|-------------|
| LoggedInFrom | Member | Device | Login events |
| OpenedGame | Member | Game | GameOpened events |
| SawPopup | Member | Popup | PopupModule events |
| HitError | Member | Error | API Error events |
| BelongsToGroup | Member | VipGroup | Login events |
| ReferredBy | Member | Affiliate | Login events |
| UsesCurrency | Member | Currency | Login events |
| PlaysOnPlatform | Member | Platform | Login events |
| ProvidedBy | Game | Provider | GameOpened events |

## Event Types (4)

| Event | Key Fields |
|-------|-----------|
| Login | member_code, device_id, vip_group, affiliate_code, currency, platform |
| GameOpened | member_code, game_code, provider_code |
| PopupModule | member_code, popup_name |
| API Error | member_code, error_code |

## Feature Vector (10 Dimensions)

Defined in `data/rules/features/feature-config.yml`:

| Dimension | Type | Description |
|-----------|------|-------------|
| login_count | count | Total login events |
| game_count | count | Total game opens |
| unique_games | count_distinct | Unique games played |
| error_count | count | Total errors |
| popup_count | count | Total popups seen |
| platform_mobile_ratio | ratio | Mobile vs total logins |
| session_count | count | Distinct sessions |
| avg_session_gap_hours | average | Mean time between sessions |
| vip_group | encoded | bronze→1, silver→2, gold→3, platinum→4, diamond→5, vip→6 |
| currency | encoded | USD→1, EUR→2, GBP→3, ... IDR→10 |

## Entity Extraction

Performed by `crates/connector/src/entity_extract.rs`:

1. Parse document fields based on event type
2. Create entity nodes for each extracted entity
3. Create edges between entities based on event type
4. Entities are upserted — same entity_id across events creates richer node

## Schema Sync Tests

Tests in `crates/rules/tests/examples.rs` verify that YAML entity/edge types match Rust enum variants. When adding new entity or edge types:

1. Add to `data/rules/schema/entity-schema.yml`
2. Add variant to Rust enum in connector
3. Run tests to verify sync
