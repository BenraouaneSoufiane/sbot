---
name: portfolio-balance
description: Check wallet portfolio and balances, including total USD value, individual token balances, fee reserve, day-over-day changes, significant moves, and expected deposit or withdrawal arrivals.
---

# portfolioBalance

Gather wallet and market facts with the `http_request` and `shell` tools, then answer the specific portfolio question.

1. Wallet holdings: call the Solana JSON-RPC `getBalance` and `getTokenAccountsByOwner` methods directly. Read `SOLANA_RPC_URL` from the environment; if unset, use `https://api.mainnet-beta.solana.com`. Use commitment `confirmed`. Query both the SPL Token program (`TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA`) and Token-2022 (`TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb`). Normalize token accounts into `{"mint","token_account","amount","decimals","ui_amount"}`.
2. Prices: call `{JUPITER_API_URL}/price/v3?ids=<comma-separated mints>` (default base `https://api.jup.ag`). If `JUPITER_API_KEY` is set, send the header `x-api-key`. Limit 1 to 50 mints per call. Cross-check with Raydium pool data via `{RAYDIUM_API_URL}/pools/info/ids?ids=<poolIds>` (default base `https://api-v3.raydium.io`; the legacy price map at `https://api.raydium.io/v2/main/price` is also usable when reachable), and with Orca via `{ORCA_API_URL}/v2/solana/pools?token=<SYMBOL>` (default base `https://api.orca.so`), which lists pools with prices and volumes.
3. Display names: call `{JUPITER_API_URL}/tokens/v2/search?query=<mint>` when a token name or symbol is needed.
4. Arrivals and day-over-day changes: call `{HELIUS_API_URL}/v0/addresses/<wallet>/transactions?api-key=<HELIUS_API_KEY>&limit=<n>` (default base `https://api-mainnet.helius-rpc.com`, default limit 20), or read recent signatures from the Solana JSON-RPC, and compare timestamps and amounts. Include Raydium, Orca, and Drift program interactions when they show deposits, withdrawals, or position changes.

Compute the total USD value from the exact holdings and current prices. Report SOL, each token (mint plus symbol), and the USD total. For comparisons use only transactions you actually observed; never fabricate a baseline. State that you observed a deposit or withdrawal only when the transaction data shows it.