---
name: get-market-data
description: Retrieve current Jupiter price/market data for one or more Solana token mints directly from Jupiter.
---

# getMarketData

Call Jupiter's price endpoint directly with the `http_request` tool:

- Method: `GET`
- URL: `{JUPITER_API_URL}/price/v3?ids=<comma-separated mints>`
- Default base when `JUPITER_API_URL` is unset: `https://api.jup.ag`
- Limit: 1 to 50 token addresses per call.
- If `JUPITER_API_KEY` is set in the environment, send the header `x-api-key: <key>`.

Return `{"skill":"getMarketData","provider":"jupiter","data":<response>}`.

Preserve provider units, do not treat missing tokens as zero-priced, and do not calculate values from stale facts. For a change over time, compare against a previous observation you already hold in memory; do not fabricate a baseline.
