---
name: get-liquidity
description: Retrieve Birdeye exit-liquidity and trading facts for a Solana token directly.
---

# getLiquidity

Call Birdeye's exit-liquidity endpoint directly with the `http_request` tool:

- Method: `GET`
- URL: `{BIRDEYE_API_URL}/defi/v3/token/exit-liquidity?address=<mint>`
- Default base when `BIRDEYE_API_URL` is unset: `https://public-api.birdeye.so`
- Required headers: `X-API-KEY: <BIRDEYE_API_KEY>` and `x-chain: solana`.

Return `{"skill":"getLiquidity","provider":"birdeye","data":<response>}`.

State the provider and timestamp when present. Liquidity is a point-in-time observation; do not infer a guaranteed execution price or safety from liquidity alone.
