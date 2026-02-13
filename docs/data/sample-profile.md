# Sample Data Profile

## Source

```
Location: D:\w88_data\
Total size: 104 GB
Total parquet files: 840
Date range: June 2025 — October 2025
```

## Directory Structure

```
D:\w88_data\
├── API Error/              # API error events (daily parquet)
│   ├── 2025-06-12.parquet
│   ├── 2025-06-13.parquet
│   └── ...
├── Login/                  # Login events (daily parquet)
│   ├── 2025-06-12.parquet
│   └── ...
├── GameOpened/             # Game open events (daily parquet)
│   ├── 2025-06-12.parquet
│   └── ...
├── PopupModule/            # Popup interaction events (daily parquet)
│   ├── 2025-06-12.parquet
│   └── ...
├── v2/                     # Newer event format (Sep-Oct 2025)
│   ├── GridClick/
│   │   ├── 2025-09-19.parquet
│   │   └── ...
│   └── PopUpModule/
│       ├── 2025-10-23.parquet
│       └── ...
├── Reco/                   # Recommendation data
│   └── badezimmer/
├── ApiVx/                  # API version data
├── apicrm_db.sql.gz        # CRM database dump (compressed)
├── apicrm_db.sql/          # CRM database dump (extracted)
├── apired_db.sql.gz        # Redis/operational DB dump (compressed)
├── apired_db.sql/          # Redis/operational DB dump (extracted)
├── strapi-crm.tar          # Strapi CMS (CRM)
├── strapi-red.tar          # Strapi CMS (Red)
├── temp-sync/              # Temporary sync files
└── New folder/             # Misc
```

## Event Type Details

### Login
| Property | Value |
|----------|-------|
| Rows per day | ~57,000 |
| Columns | 72 |
| All types | string |
| File size per day | ~15 MB |

**Key columns**: `memberCode`, `success`, `method`, `platform`, `currency`, `rGroup`, `fingerprint`, `ipAddress`, `@timestamp`, `device`, `os`, `osVersion`, `deviceModel`, `userAgent`, `lang`, `language`, `isTestAccount`

**Sample row**:
```
memberCode: thongtran2904
platform: web-android
currency: VND
rGroup: VIPB
success: true
method: username
@timestamp: 2025-06-12T23:59:58.997Z
fingerprint: 790e408dc6cc12b06b467757d9ec3762
device: mobile
```

### GameOpened
| Property | Value |
|----------|-------|
| Rows per day | ~308,000 |
| Columns | 80 |
| All types | string |
| File size per day | ~80 MB |

**Key columns**: `memberCode`, `game`, `gameUid`, `category`, `gameTrackingProvider`, `gameTrackingCategory`, `gameTrackingId`, `platform`, `currency`, `rGroup`, `from`, `componentId`, `@timestamp`

**Sample row**:
```
memberCode: vominhsang543
game: retrow88
category: Slots
gameTrackingProvider: GPI
platform: web-android
currency: VND
rGroup: VIPB
from: /slots
gameUid: 016c732b-80d8-472e-9916-abb2b30a698c
@timestamp: 2025-06-12T23:59:59.997Z
```

### API Error
| Property | Value |
|----------|-------|
| Rows per day | ~421,000 |
| Columns | 155 |
| All types | string |
| File size per day | ~200 MB |
| Note | Most columns are sparse (null) per row |

**Key columns**: `error`, `stage`, `platform`, `memberCode`, `page`, `method`, `status`, `@timestamp`

**Note**: Many column names contain embedded HTML error responses, indicating data quality issues in the source pipeline. Column names like `<head><title>404 Not Found</title></head>...` exist.

### PopupModule
| Property | Value |
|----------|-------|
| Rows per day | ~172,000 |
| Columns | 60 |
| All types | string |
| File size per day | ~40 MB |

**Key columns**: `memberCode`, `popupType`, `clickType`, `componentId`, `displayTime`, `isManual`, `game`, `platform`, `currency`, `@timestamp`

## Daily Volume Summary

| Event Type | Rows/Day | Columns | Est. Size/Day |
|-----------|----------|---------|---------------|
| API Error | 421,000 | 155 | 200 MB |
| GameOpened | 308,000 | 80 | 80 MB |
| PopupModule | 172,000 | 60 | 40 MB |
| Login | 57,000 | 72 | 15 MB |
| **Total** | **958,000** | — | **~335 MB** |

## Common Fields (Join Keys)

Fields present across most/all event types:

| Field | Normalized Name | Entity Type | Cardinality |
|-------|----------------|-------------|-------------|
| `memberCode` | `member_code` | Member | ~50K/day |
| `fingerprint` | `fingerprint` | Device | ~30K/day |
| `platform` | `platform` | Platform | ~8 values |
| `currency` | `currency` | Currency | 3: VND, THB, IDR |
| `rGroup` | `vip_group` | VipGroup | ~6 values |
| `affiliateId` / `affiliateid` / `affiliateID` | `affiliate_id` | Affiliate | ~500 |
| `@timestamp` | `timestamp` | — | Continuous |
| `eventName` | `event_name` | — | ~10 types |
| `stage` | `stage` | — | Pre-Production, etc. |
| `isTestAccount` | `is_test_account` | — | true/false |
| `spfid` | `spfid` | — | Session ID |

## Data Quality Issues

1. **All string types** — no numeric or boolean columns, everything is text
2. **Inconsistent field names** — `affiliateId`, `affiliateid`, `affiliateID` all mean the same thing
3. **HTML in column names** — API Error parquet has column names containing HTML error responses
4. **Null representations** — `None`, `null`, empty string, `undefined` all represent null
5. **Sparse schema** — API Error has 155 columns but most are null for any given row
6. **Mixed platform formats** — both `platform` field and device-specific fields describe the client
7. **Duplicate data paths** — `PopupModule/` at top level and `v2/PopUpModule/` with slightly different schemas

## SQL Dumps

### apicrm_db.sql
CRM database — likely contains:
- Member profiles, registration data
- VIP tier history
- Affiliate relationships

### apired_db.sql
Operational database — likely contains:
- Session data
- Real-time state
- Configuration

These could be imported as **reference data** to enrich the event graph with static member attributes.
