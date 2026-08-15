---
name: get-protocol-events
description: Retrieve recent Helius-enhanced Solana transactions and protocol events for an address directly.
---

# getProtocolEvents

Call Helius's transaction endpoint directly with the `http_request` tool:

- Method: `GET`
- URL: `{HELIUS_API_URL}/v0/addresses/<address>/transactions?api-key=<HELIUS_API_KEY>&limit=<limit>`
- Default base when `HELIUS_API_URL` is unset: `https://api-mainnet.helius-rpc.com`
- Limit: 1 to 100; omit it to use 20.

Return `{"skill":"getProtocolEvents","provider":"helius","address":"<address>","events":<response>}`.

Summarize returned events chronologically and distinguish observed facts from interpretation. Do not claim the response is a complete history.
