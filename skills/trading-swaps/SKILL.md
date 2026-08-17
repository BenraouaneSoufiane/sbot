---
name: trading-swaps
description: Answer trading questions such as token prices, planned swap values, swap route comparison, execution, slippage, recent trading activity, and realized or unrealized P&L.
---

# tradingSwaps

Gather market and wallet facts with the `http_request` and `shell` tools, then answer the trading question.

1. Prices: call `{JUPITER_API_URL}/price/v3?ids=<comma-separated mints>` (default base `https://api.jup.ag`). If `JUPITER_API_KEY` is set, send the header `x-api-key`. Limit 1 to 50 mints per call. Cross-check with Raydium pool data via `{RAYDIUM_API_URL}/pools/info/ids?ids=<poolIds>` (default base `https://api-v3.raydium.io`) and Orca pools via `{ORCA_API_URL}/v2/solana/pools?token=<SYMBOL>` (default base `https://api.orca.so`).
2. Swap quotes and routes: call `{JUPITER_API_URL}/quote` (default base `https://api.jup.ag`) with the input mint, output mint, amount, and slippage; compare routes when asked. For Raydium-specific quotes, use Raydium's pool data and route endpoints; for Orca, use the Whirlpool swap instructions. For an actual swap, use Jupiter's swap API only with the user's explicit approval and a funded, user-authorized wallet.
3. Recent trading activity: call `{HELIUS_API_URL}/v0/addresses/<wallet>/transactions?api-key=<HELIUS_API_KEY>&limit=<n>` (default base `https://api-mainnet.helius-rpc.com`, default limit 20), including Raydium, Orca, and Drift fills.
4. Perp markets (Drift): call `{DRIFT_API_URL}/markets` and `{DRIFT_API_URL}/perpMarketInfo?marketIndex=<n>` (default base `https://api.drift.trade`) for market prices and indexes, and `{DRIFT_API_URL}/fundingRates?marketIndex=<n>` for funding; `{DRIFT_API_URL}/user?authority=<wallet>` shows open perp positions for realized/unrealized P&L.
5. P&L: combine observed swap fills with current prices.

Never execute a swap without explicit user confirmation. Report prices and quotes as point-in-time facts. Compute realized or unrealized P&L only from observed fills and current prices; never invent a cost basis.