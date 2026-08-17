---
name: security-monitoring
description: Monitor a wallet for security issues such as new approvals or allowances, unexpected outgoing transfers, unfamiliar contracts, large transfers, low fee balances, and unexpected incoming tokens.
---

# securityMonitoring

Inspect the wallet's on-chain state with the `http_request` and `shell` tools, then answer the security question.

1. Activity: call `{HELIUS_API_URL}/v0/addresses/<wallet>/transactions?api-key=<HELIUS_API_KEY>&limit=<n>` (default base `https://api-mainnet.helius-rpc.com`, default limit 20). Recognize Raydium, Orca, and Drift program interactions and flag activity that comes from any other, unfamiliar program.
2. Fee reserve: call the Solana JSON-RPC `getBalance` with commitment `confirmed` from `SOLANA_RPC_URL` (default `https://api.mainnet-beta.solana.com`).
3. Token accounts and received tokens: call `getTokenAccountsByOwner` for both the SPL Token program (`TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA`) and Token-2022 (`TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb`) with `{"encoding":"jsonParsed","commitment":"confirmed"}`.
4. Drift account: if the wallet trades on Drift, call `{DRIFT_API_URL}/user?authority=<wallet>` (default base `https://api.drift.trade`) to surface unexpected open positions, borrows, or collateral changes.

Look for new token approvals or allowances, outgoing transfers the user did not expect, interactions with unfamiliar programs or contracts, large transfers, a SOL balance too low for fees, and newly received tokens. Distinguish observed facts from interpretation; when a program is unfamiliar, say so rather than judging it. Never invent approvals, transfers, or balances.