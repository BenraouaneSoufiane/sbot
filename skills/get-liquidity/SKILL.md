---
name: get-liquidity
description: Retrieve Birdeye liquidity and trading facts for a Solana token. Use for liquidity checks, trading depth context, volume, activity, or risk analysis.
---

# getLiquidity

Call the Rust backend with the `http_request` tool:

- Method: `POST`
- URL: `${RECONSILE_API_BASE_URL}/api/skills/getLiquidity`
- Default base URL when not configured: `http://127.0.0.1:4173`
- JSON body: `{"token_address":"<base58 Solana mint address>"}`

State the provider and timestamp when present. Explain that liquidity is a point-in-time observation; do not infer guaranteed execution price or safety from liquidity alone.
