# Data Profile — Sample Dataset

## Source

Location: `D:\w88_data\`
Total size: **104 GB**
Files: **840 parquet files**
Date range: June 2025 — October 2025

## Event Types

### Login Events
- **Path**: `D:/w88_data/Login/YYYY-MM-DD.parquet`
- **Rows per day**: ~57,000
- **Columns**: 72
- **Key fields**:
  - `memberCode` — User identifier
  - `success` — "true" / "false"
  - `method` — "username", etc.
  - `platform` — "web-android", "web-ios", "android", "ios"
  - `currency` — "VND", "THB", "IDR"
  - `rGroup` — VIP group: "VIPB", "VIPP", "VIPG"
  - `fingerprint` — Device fingerprint hash
  - `ipAddress` — Client IP
  - `@timestamp` — Event time (ISO 8601)

### GameOpened Events
- **Path**: `D:/w88_data/GameOpened/YYYY-MM-DD.parquet`
- **Rows per day**: ~308,000
- **Columns**: 80
- **Key fields**:
  - `memberCode` — User identifier
  - `game` — Game slug (e.g., "retrow88", "prosperitywheel")
  - `gameUid` — Unique game instance ID (UUID)
  - `category` — "Slots", "Live Casino", "Sports", etc.
  - `gameTrackingProvider` — "GPI", etc.
  - `gameTrackingCategory` — Game sub-category
  - `from` — Referral source within app (e.g., "/slots", "popup")
  - `componentId` — UI component that triggered the game open

### API Error Events
- **Path**: `D:/w88_data/API Error/YYYY-MM-DD.parquet`
- **Rows per day**: ~421,000
- **Columns**: 155 (most are sparse/null)
- **Key fields**:
  - `error` — Error message/code
  - `stage` — "Pre-Production", etc.
  - `platform` — Same as Login
  - `memberCode` — May be null (unauthenticated errors)
  - `page` — Page where error occurred
  - `method` — HTTP method or auth method
  - `status` — HTTP status code

### PopupModule Events
- **Path**: `D:/w88_data/PopupModule/YYYY-MM-DD.parquet`
- **Rows per day**: ~172,000
- **Columns**: 60
- **Key fields**:
  - `memberCode` — User identifier
  - `popupType` — Type of popup shown
  - `clickType` — User interaction type
  - `componentId` — Popup component identifier
  - `displayTime` — How long popup was shown
  - `isManual` — Whether popup was manually triggered
  - `game` — Associated game (if any)

### V2 Events (GridClick, PopUpModule)
- **Path**: `D:/w88_data/v2/GridClick/`, `D:/w88_data/v2/PopUpModule/`
- **Date range**: September — October 2025
- Newer format/version of similar event types

## Common Fields Across All Events

These fields appear in most/all event types and are the primary join keys:

| Field | Description | Entity Type |
|-------|-------------|-------------|
| `memberCode` | User identifier | Member |
| `fingerprint` | Device fingerprint | Device |
| `platform` | Client platform | Platform |
| `currency` | User currency | Currency |
| `rGroup` | VIP tier | VipGroup |
| `affiliateId` / `affiliateid` / `affiliateID` | Affiliate reference | Affiliate |
| `@timestamp` | Event timestamp | (time dimension) |
| `eventName` | Event type | (event type) |
| `stage` | Environment | (metadata) |
| `isTestAccount` | Test account flag | (filter) |

## Schema Observations

1. **All fields are strings** — no typed columns in the parquet files. Type inference needed at ingestion.
2. **Field name inconsistency** — `affiliateId`, `affiliateid`, `affiliateID` are the same field. Normalization needed.
3. **Sparse columns** — API Error has 155 columns but most are null for any given row. Storage should handle sparsity well.
4. **Embedded HTML** — Some column names contain HTML error responses (appears to be a data quality issue in the source).
5. **Mixed sources** — Some events come from PWA (`pwaVersion`, `buildVersion`), some from native apps (`appMemoryInfo.*`), some from web.

## Volume Projections

| Metric | Sample (per day) | Full Scale Estimate |
|--------|-------------------|---------------------|
| Total events | ~958,000 | ~5,000,000 |
| Total data size | ~3.5 GB | ~100-170 GB |
| Unique members | ~50,000 | ~200,000 |
| Unique games | ~500 | ~2,000 |
| 30-day total | ~29M events | ~150M events |
| 30-day storage | ~104 GB | ~3-5 TB |

## Other Data Sources

The sample also contains:
- `apicrm_db.sql.gz` — CRM database SQL dump
- `apired_db.sql.gz` — Redis/operational database SQL dump
- `strapi-crm.tar` — Strapi CMS application
- `strapi-red.tar` — Another Strapi instance

These are relational/CMS data that could be ingested as reference data (member profiles, game catalog, etc.) to enrich the event graph.
