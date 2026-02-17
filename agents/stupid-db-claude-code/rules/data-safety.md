# Data Safety Rules

## Production Data
- D:\w88_data is read-only production sample data (104GB, ~960K events/day)
- NEVER modify, write to, or delete files in this directory
- Always analyze sample parquet files before changing entity extraction model

## Credentials & Secrets
- NEVER commit .env files, API keys, or credentials to git
- Use three-tier credential system: Config → Safe → Credentials
- All credentials encrypted at rest using AES-256-GCM with auto-generated keys
- API responses must use Safe variant with masked passwords

## Storage
- Segment storage uses 15-30 day rolling window with TTL eviction
- Design for continuous eviction — NEVER append-only
- Segment lifecycle: Active (writing) → Sealed (read-only) → Archived → Evicted
- O(1) eviction via segment drop — no compaction needed
