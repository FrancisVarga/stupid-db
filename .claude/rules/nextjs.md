# Next.js Dashboard Rules

- Dashboard is chat-first interface, not traditional BI panels with dropdowns/filters
- Use D3.js for visualizations, not Chart.js or other libraries
- No authentication required — assume internal/trusted network deployment
- Use refreshKey pattern for parent-child component sync — parent increments key, child useEffect re-fetches

## API Boundary Safety
- For API responses with optional numeric fields, use `?? 0` guards when performing math operations or formatting — TypeScript interfaces don't enforce runtime shapes; Rust `Option<u64>` serializes as `null`, causing TypeError on `.toFixed()`, `Math.round()`, etc.
