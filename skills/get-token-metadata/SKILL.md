---
name: get-token-metadata
description: Retrieve metadata for a Solana token mint directly from Jupiter.
---

# getTokenMetadata

Call Jupiter's token search endpoint directly with the `http_request` tool:

- Method: `GET`
- URL: `{JUPITER_API_URL}/tokens/v2/search?query=<mint>`
- Default base when `JUPITER_API_URL` is unset: `https://api.jup.ag`
- If `JUPITER_API_KEY` is set in the environment, send the header `x-api-key: <key>`.

Return `{"skill":"getTokenMetadata","provider":"jupiter","data":<response>}`.

Use only the metadata returned by Jupiter. Keep the mint address in summaries when token names or symbols could be ambiguous.
