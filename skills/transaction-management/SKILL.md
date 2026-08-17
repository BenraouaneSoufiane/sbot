---
name: transaction-management
description: Review a wallet's transactions, categorize them, find failed, pending, large, or unusual ones, verify transfers, and reconcile on-chain activity against an internal ledger.
---

# transactionManagement

Fetch the wallet's recent on-chain activity with the `http_request` and `shell` tools, then answer the transaction question.

1. Primary: call `{HELIUS_API_URL}/v0/addresses/<wallet>/transactions?api-key=<HELIUS_API_KEY>&limit=<n>` (default base `https://api-mainnet.helius-rpc.com`; limit 1 to 100, omit for 20).
2. Fallback: if Helius is unavailable, use the Solana JSON-RPC `getSignaturesForAddress` and `getTransaction` methods with `SOLANA_RPC_URL` (default `https://api.mainnet-beta.solana.com`).

For each transaction note the signature, timestamp, type (swap, transfer, deposit, withdrawal, payment, staking), amount, counterparties, and status when determinable (success, failed, pending). Recognize Raydium (AMM/CLMM swaps, farm deposits), Orca (Whirlpool swaps, LP actions), and Drift (perp fills, deposits, borrows) program interactions and categorize them accordingly. Categorize as requested, flag failed or reverted transactions, list pending items, verify a transfer reached the expected wallet, reconcile the observed activity against an internal ledger or statement the user provides, and surface transactions above the requested amount. Never claim a transaction is confirmed unless you observed its status, and never invent ledger records.