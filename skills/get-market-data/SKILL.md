---
name: get-market-data
description: Retrieve current Jupiter market data plus skill-managed previous/current snapshots and metric deltas for one or more Solana token mints.
---

# getMarketData

Call the Rust backend with the `http_request` tool:

- Method: `POST`
- URL: `${RECONSILE_API_BASE_URL}/api/skills/getMarketData`
- Default base URL when not configured: `http://127.0.0.1:4173`
- JSON body: `{"token_addresses":["<base58 mint>"]}`
- Limit: 1 to 50 token addresses per call.

The endpoint atomically maintains a rolling baseline for every mint and returns:

- `data`: current Jupiter facts.
- `snapshots.previous`: the preceding captured facts, when available.
- `snapshots.current`: the facts captured by this call.
- `snapshots.comparison`: per-mint changes for price, liquidity, 24-hour price change, volume, and volatility when both snapshots contain those metrics.

Use `snapshots.comparison` for comparisons instead of asking the model, memory, or another tool for a previous market snapshot. A false `baselineAvailable` means this call established the first baseline; do not claim that snapshot storage is missing. Report only that comparison begins with the next observation. Preserve provider units, do not treat missing tokens as zero-priced, and do not calculate values from stale facts.
