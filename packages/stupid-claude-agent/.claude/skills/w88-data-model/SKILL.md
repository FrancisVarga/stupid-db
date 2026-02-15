---
name: w88 Data Model
description: Complete w88 gaming/analytics data model for stupid-db. Entity types, event types, join keys, field mappings, data volumes, and relationship structure. Use when working with the data domain, entity extraction, or understanding event patterns.
version: 1.0.0
---

# w88 Data Model

## Data Overview

| Property | Value |
|----------|-------|
| Source | w88 gaming platform event logs |
| Format | Parquet files |
| Sample location | D:\w88_data (104GB, READ-ONLY) |
| Daily volume | ~960,000 events |
| Retention | 15-30 day rolling window |
| Production scale | 3-5TB |

## Event Types

### Login (~57,000/day)
Player authentication events.

| Field | Type | Description |
|-------|------|-------------|
| `memberCode` | string | Player account ID |
| `fingerprint` | string | Browser/device fingerprint |
| `ip` | string | IP address |
| `userAgent` | string | Browser user agent |
| `platform` | string | mobile/desktop |
| `vipGroup` | string | VIP tier |
| `currency` | string | Player currency |
| `affiliateId` | string | Referrer ID |
| `@timestamp` | datetime | Event time |

### GameOpened (~308,000/day)
Game session start events. Highest volume event type.

| Field | Type | Description |
|-------|------|-------------|
| `memberCode` | string | Player account ID |
| `gameUid` | string | Unique game identifier |
| `gameName` | string | Human-readable game name |
| `provider` | string | Game provider company |
| `platform` | string | mobile/desktop |
| `@timestamp` | datetime | Event time |

### API Error (~421,000/day)
Backend API error events. Highest absolute volume.

| Field | Type | Description |
|-------|------|-------------|
| `memberCode` | string | Player account ID (if authenticated) |
| `errorCode` | string | Error code identifier |
| `errorMessage` | string | Error description |
| `apiPath` | string | API endpoint path |
| `platform` | string | mobile/desktop |
| `@timestamp` | datetime | Event time |

### PopupModule (~172,000/day)
Popup/promotion display events.

| Field | Type | Description |
|-------|------|-------------|
| `memberCode` | string | Player account ID |
| `rGroup` | string | Popup/promotion group |
| `popupType` | string | Type of popup |
| `@timestamp` | datetime | Event time |

## Entity Model (10 Types)

| Entity | Source Field | Cardinality (est.) | Description |
|--------|------------|-------------------|-------------|
| Member | `memberCode` | ~50,000 | Player accounts |
| Device | `fingerprint` | ~80,000 | Browser fingerprints |
| Game | `gameUid` | ~5,000 | Game instances |
| Popup | `rGroup` | ~200 | Promotion groups |
| Error | `errorCode` | ~500 | API error codes |
| VipGroup | `vipGroup` | ~20 | VIP tiers |
| Affiliate | `affiliateId` | ~1,000 | Referrers |
| Currency | `currency` | ~30 | Player currencies |
| Platform | `platform` | ~5 | Device platforms |
| Provider | `provider` | ~100 | Game providers |

## Relationships (Edge Types)

| Edge | From → To | Source Event | Meaning |
|------|-----------|-------------|---------|
| PLAYS | Member → Game | GameOpened | Member played this game |
| USES_DEVICE | Member → Device | Login | Member logged in from this device |
| SAW_POPUP | Member → Popup | PopupModule | Member saw this promotion |
| HAS_ERROR | Member → Error | API Error | Member encountered this error |
| IN_VIP_GROUP | Member → VipGroup | Login | Member's VIP tier |
| REFERRED_BY | Member → Affiliate | Login | Member's referrer |
| USES_CURRENCY | Member → Currency | Login | Member's currency |
| ON_PLATFORM | Member → Platform | Any | Member's device platform |
| PROVIDED_BY | Game → Provider | GameOpened | Game's provider company |

## Join Keys

| Key | Primary Entity | Connected Entities | Used In |
|-----|---------------|-------------------|---------|
| `memberCode` | Member | All event-linked entities | All events |
| `fingerprint` | Device | Member | Login only |
| `gameUid` | Game | Member, Provider | GameOpened |
| `rGroup` | Popup | Member | PopupModule |
| `errorCode` | Error | Member | API Error |
| `affiliateId` | Affiliate | Member | Login |
| `currency` | Currency | Member | Login |
| `@timestamp` | (temporal) | All | Temporal joins |

## Common Analytical Questions

| Business Question | Data Approach |
|-------------------|---------------|
| "Which members are most active?" | PageRank on Member nodes or count GameOpened per member |
| "Are there shared devices (fraud)?" | Graph: Device nodes with >1 Member edge |
| "Which games are trending?" | Trend detection on GameOpened counts per gameUid |
| "Error hotspots?" | Co-occurrence: Error × Game or Error × Platform |
| "Member segments?" | KMeans on member feature vectors |
| "Popup effectiveness?" | Temporal: PopupSeen → GameOpened sequence frequency |
| "Churn risk?" | Anomaly detection on member activity decline |
| "Affiliate quality?" | PageRank/stats on Affiliate → Member → activity chains |

## Data Quality Notes

- `memberCode` can be null for unauthenticated API errors
- `fingerprint` only present in Login events
- `gameUid` format varies by provider
- Timestamps are UTC
- Some fields have inconsistent casing across event types (normalization needed)

## SAFETY: Never modify D:\w88_data
