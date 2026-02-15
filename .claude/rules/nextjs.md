# Next.js Dashboard Rules

- Dashboard is chat-first interface, not traditional BI panels with dropdowns/filters
- Use D3.js for visualizations, not Chart.js or other libraries
- No authentication required — assume internal/trusted network deployment
- Use refreshKey pattern for parent-child component sync — parent increments key, child useEffect re-fetches
