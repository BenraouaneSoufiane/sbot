---
name: get-wallet-holdings
description: Retrieve normalized SOL and SPL token holdings for a Solana wallet. Use when a user asks what a wallet owns, its balances, portfolio contents, or token positions.
---

# getWalletHoldings

Call the Rust backend with the `http_request` tool:

- Method: `POST`
- URL: `${RECONSILE_API_BASE_URL}/api/skills/getWalletHoldings`
- Default base URL when not configured: `http://127.0.0.1:4173`
- JSON body: `{"wallet_address":"<base58 Solana wallet address>"}`

Treat the returned `native_balance` and `holdings` as facts. Do not invent prices or fiat values; call `getMarketData` separately when valuation is requested. Surface backend errors clearly.
