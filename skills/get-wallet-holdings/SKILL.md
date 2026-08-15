---
name: get-wallet-holdings
description: Retrieve normalized SOL and SPL token holdings for a Solana wallet by querying the Solana JSON-RPC directly.
---

# getWalletHoldings

Query the Solana JSON-RPC directly with the `http_request` tool. Read `SOLANA_RPC_URL` from the environment (via the `shell` tool if needed); if it is unset, use `https://api.mainnet-beta.solana.com`.

Make three JSON-RPC POST calls (content-type `application/json`, body `{"jsonrpc":"2.0","id":N,"method":"...","params":[...]}`), all with `"commitment":"confirmed"`:

1. `getBalance` with params `[<wallet>, {"commitment":"confirmed"}]` — native SOL. `result.value` is lamports; SOL = lamports / 1_000_000_000.
2. `getTokenAccountsByOwner` with params `[<wallet>, {"programId":"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"}, {"encoding":"jsonParsed","commitment":"confirmed"}]` — SPL Token accounts.
3. The same call with `"programId":"TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"` — Token-2022 accounts.

Normalize token accounts from `result.value[].account.data.parsed.info` into `{"mint","token_account","amount","decimals","ui_amount"}`, taking `amount`/`decimals`/`ui_amount` from `tokenAmount`. Combine the two token lists.

Return a single object:
`{"skill":"getWalletHoldings","wallet_address":"<wallet>","native_balance":{"lamports":n,"sol":n/1e9},"holdings":[...],"provider":"solana-rpc"}`.

Treat the returned balances as facts. Do not invent prices or fiat values; call getMarketData separately when valuation is requested. Surface RPC errors clearly.
