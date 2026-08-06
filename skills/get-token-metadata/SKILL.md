---
name: get-token-metadata
description: Retrieve metadata for a Solana token mint through the Rust backend. Use to identify a mint, symbol, name, decimals, logo, tags, or verification details.
---

# getTokenMetadata

Call the Rust backend with the `http_request` tool:

- Method: `POST`
- URL: `${RECONSILE_API_BASE_URL}/api/skills/getTokenMetadata`
- Default base URL when not configured: `http://127.0.0.1:4173`
- JSON body: `{"token_address":"<base58 Solana mint address>"}`

Use only metadata returned by the backend. Keep the mint address in summaries when token names or symbols could be ambiguous.
