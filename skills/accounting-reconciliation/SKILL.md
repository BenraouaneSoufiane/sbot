---
name: accounting-reconciliation
description: Reconcile a wallet against records, find missing or duplicate transactions, match transfers between wallets, compute daily inflows and outflows, generate treasury reports, calculate realized gains and losses, and export transactions.
---

# accountingReconciliation

Reconcile a wallet's on-chain activity against recorded data with the `http_request` and `shell` tools, then answer the accounting question.

1. On-chain activity: call `{HELIUS_API_URL}/v0/addresses/<wallet>/transactions?api-key=<HELIUS_API_KEY>&limit=<n>` (default base `https://api-mainnet.helius-rpc.com`, default limit 20); fall back to the Solana JSON-RPC `getSignaturesForAddress`/`getTransaction` from `SOLANA_RPC_URL` (default `https://api.mainnet-beta.solana.com`).
2. Balances: call the Solana JSON-RPC `getBalance` and `getTokenAccountsByOwner` (SPL `TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA` and Token-2022 `TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb`) for the wallet or wallets.
3. Records: use any ledger, statement, or source the user provides for the comparison.

Cross-reference each recorded transaction against on-chain activity to find transactions missing from the accounting system, accounting entries with no on-chain match, transfers between two wallets, duplicates, and daily inflows or outflows. Include Raydium, Orca, and Drift positions and flows (LP deposits/withdrawals, Whirlpool activity, Drift deposits/borrows/funding) in the reconciliation when they affect the wallet's balances. Generate a daily treasury report and calculate realized gains and losses only from observed transactions and amounts. When asked to export, format the transactions as CSV. Never invent ledger entries or transaction facts.