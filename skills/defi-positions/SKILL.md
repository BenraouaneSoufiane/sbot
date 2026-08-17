---
name: defi-positions
description: Check DeFi positions such as staking, liquidity-provider positions, lending and borrowing, collateral ratios, liquidation proximity, farming rewards, APYs, and rebalancing needs.
---

# defiPositions

Assess the wallet's DeFi positions with the `http_request` and `shell` tools, then answer the specific question.

1. Protocol activity: call `{HELIUS_API_URL}/v0/addresses/<wallet>/transactions?api-key=<HELIUS_API_KEY>&limit=<n>` (default base `https://api-mainnet.helius-rpc.com`, default limit 20) to see staking, LP, lending, and farming interactions.
2. Raydium pools and farms: call `{RAYDIUM_API_URL}/pools/info/ids?ids=<poolIds>` and `{RAYDIUM_API_URL}/farms/info/ids?ids=<farmIds>` (default base `https://api-v3.raydium.io`) for pool reserves, prices, APRs, and farm rewards; combine with `getTokenAccountsByOwner` to find LP and farm token balances.
3. Orca Whirlpool positions: call `{ORCA_API_URL}/v2/solana/pools/search?q=<TOKEN>` and `{ORCA_API_URL}/v2/solana/pools/<address>` (default base `https://api.orca.so`) for price, volume, fee tier, and APR; position NFTs show up in the wallet's token accounts and Helius events.
4. Drift positions: call `{DRIFT_API_URL}/user?authority=<wallet>` (default base `https://api.drift.trade`) for the user account, collateral, open perp positions, and borrows; `{DRIFT_API_URL}/markets`, `{DRIFT_API_URL}/perpMarketInfo?marketIndex=<n>`, and `{DRIFT_API_URL}/fundingRates?marketIndex=<n>` provide market, oracle, and funding data for collateral and liquidation checks.
5. Position balances: call the Solana JSON-RPC `getTokenAccountsByOwner` for both the SPL Token program (`TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA`) and Token-2022 (`TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb`) from `SOLANA_RPC_URL` (default `https://api.mainnet-beta.solana.com`).
6. Valuation: call `{JUPITER_API_URL}/price/v3?ids=<mints>` (default base `https://api.jup.ag`; send `x-api-key` when `JUPITER_API_KEY` is set) for collateral and position pricing.

Report observed positions, token amounts, and prices. For collateral ratios, liquidation thresholds, APYs, and reward rates, use only values you actually retrieved from the protocol or its transactions; otherwise state that the exact protocol parameter is not available rather than guessing. When a claim or position is at risk, say which fact signals that risk.