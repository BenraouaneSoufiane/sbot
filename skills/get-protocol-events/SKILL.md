---
name: get-protocol-events
description: Retrieve recent Helius-enhanced Solana transactions and protocol events for an address. Use to explain recent wallet, token, program, swap, transfer, NFT, or DeFi activity.
---

# getProtocolEvents

Call the Rust backend with the `http_request` tool:

- Method: `POST`
- URL: `${RECONSILE_API_BASE_URL}/api/skills/getProtocolEvents`
- Default base URL when not configured: `http://127.0.0.1:4173`
- JSON body: `{"address":"<base58 Solana address>","limit":20}`
- Limit: 1 to 100; omit it to use 20.

Summarize returned events chronologically and distinguish observed facts from interpretation. Do not claim the response is a complete history.
