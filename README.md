# Reconsile
<img width="1942" height="809" alt="ChatGPT Image Aug 6, 2026, 09_58_48 PM" src="https://github.com/user-attachments/assets/69590a36-929c-4d26-a575-5e8d37dac67a" />

Reconsile is a self-hosted reconciliation workspace for comparing operational records, tracking exceptions, and notifying the people who need to act. It combines a responsive browser UI with a thin Rust/Axum shell for state, scheduling, and the WhatsApp webhook, while a ZeroClaw agent performs all source fetching, reconciliation, notification, and Solana lookups.

The repository contains a web application, not a desktop runtime. Tauri, Slint, a webview, Node.js, and a graphical display server are not required to run it.

## Run it now (busy users)

```bash
bash scripts/env-secrets.sh materialize --builtin  # unpack the team's secrets into .env
cargo run --manifest-path src-tauri/Cargo.toml     # start the app at http://127.0.0.1:4173
```

That's it — the repository ships an encrypted copy of the team's environment in `.deploy/zeroclaw-secrets/`, so a fresh clone runs without creating API keys. After editing `.env`, refresh the shared copy so the team gets the change:

```bash
bash scripts/env-secrets.sh store --builtin   # push changed secrets into the repo copy
# then commit .deploy/zeroclaw-secrets/
```

Details, the local-`~/.zeroclaw` alternative, and the security note: [Shared environment](#shared-environment-secrets-in-zeroclaws-config).

## What it does

- Create reconciliation checks from a plain-language statement, one or more data sources, an optional Solana wallet, a schedule, and notification rules.
- Load sources from HTTP(S) endpoints with no authentication, bearer authentication, or a custom header. Built-in `demo://` sources make the initial workspace usable offline.
- Run checks manually or on hourly, daily, and weekly schedules in an IANA timezone.
- Delegate every run to a ZeroClaw agent that fetches sources, compares records, and sends notifications.
- Track run status, record counts, match rates, exceptions, detailed run logs, and notification outcomes.
- Persist checks, connections, run history, exceptions, webhook markers, and market/holdings snapshots under one data directory.
- Notify through ZeroClaw channels (email, Telegram, Discord, WhatsApp, or custom).
- Accept signed WhatsApp webhook messages that can create, edit, run, or stop checks.
- Provide five Solana skills that query Solana RPC, Jupiter, Birdeye, and Helius directly.

The UI includes overview, checks, run history, and exception views; workspace search; in-app activity notifications; check and schedule editors; connection setup; responsive mobile navigation; and run-log inspection.

## Architecture

```text
Browser
  └── prebuilt React workspace (web/)
       └── Rust HTTP service (Axum)
            ├── workspace CRUD and run APIs
            ├── 30-second schedule evaluator
            ├── atomic JSON persistence
            └── signed WhatsApp webhook controller

ZeroClaw agent (required)
  ├── fetches data sources (http_request)
  ├── reconciliation analysis
  ├── outbound notifications (channel send)
  ├── WhatsApp command classifier
  └── project skills under skills/
       └── Solana lookups (Solana RPC, Jupiter, Birdeye, Helius)
```

The Rust service serves both `/api/*` and the static SPA. Unknown browser routes fall back to `web/index.html`.

## Requirements

- A current stable Rust toolchain with Cargo.
- Network access for HTTP data sources and any configured notification or Solana providers.
- ZeroClaw (the agent runtime) installed and configured; every reconciliation run, WhatsApp command, notification, and Solana lookup depends on it.

No database or JavaScript build tool is required for the checked-in application. The compiled frontend assets are already in `web/`.

## Quick start

From the repository root:

```bash
bash scripts/env-secrets.sh materialize --builtin   # shared repo copy -> .env
cargo run --manifest-path src-tauri/Cargo.toml
```

Secret values are stored in a ZeroClaw config file — encrypted at rest with ZeroClaw's keystore — never in plaintext. A shared encrypted copy ships in the repository (`.deploy/zeroclaw-secrets/`), so `materialize --builtin` rebuilds `.env` with the team's keys right after a fresh clone. It needs bash, the `zeroclaw` CLI, and `python3` with `pip install cryptography` (only for the decrypt step — openssl's `enc` cannot do ChaCha20-Poly1305). If you prefer a blank configuration, `cp .env.example .env` instead and fill in your own values.

Open <http://127.0.0.1:4173>. The service loads `.env` without overriding variables already exported by the process.

On first start, Reconsile creates `.data/state.json` with demo checks and sample history. The demo sources are inlined into the agent prompt so the initial workspace is usable offline, but every run still requires a configured ZeroClaw agent. Change `RECONSILE_DATA_DIR` if the state should live elsewhere.

Check service health with:

```bash
curl http://127.0.0.1:4173/api/health
```

The response reports `"zeroclaw":"connected"`.

## Reconciliation

Every run is delegated to a ZeroClaw agent. Reconsile:

1. Builds a prompt from the check statement, source definitions (URL and auth), optional wallet address, and notification rules.
2. Invokes `zeroclaw agent -a <agent> -m <prompt>`.
3. The agent fetches each source with its `http_request` tool, compares the records against the statement, sends any required notifications with `zeroclaw channel send`, and returns JSON.
4. Reconsile normalizes the returned JSON into a summary, counts, and exceptions and persists the result.

The agent must return JSON containing `summary`, `records`, `matched`, and `exceptions`. Each exception may contain `title`, `detail`, `amount`, and `severity`. Notification outcomes are reported through `notifications` (channel ids) and `notificationError`.

The included `.deploy/zeroclaw.config.toml` defines a `reconcile` agent and loads the project skill bundle. Adjust its provider, channel, and risk settings for your environment before production use.

## Data sources

A check can use multiple sources. Supported URLs are:

- `https://...` and `http://...` endpoints.
- `demo://orders`, `demo://payouts`, and `demo://inventory` (inlined into the agent prompt for offline use).

Sources are fetched by the ZeroClaw agent with `GET` during each run. Authentication can be disabled, sent as `Authorization: Bearer <token>`, or supplied through a custom header; the agent is instructed on which header to use. The source tester (`POST /api/sources/test`) also runs through a ZeroClaw agent and reports a record count and a small preview before a check is saved.

JSON responses are treated as structured data; non-JSON responses, including CSV text, are retained as bounded text for analysis. Source credentials are persisted in the workspace state file, so the data directory must be protected like a secrets store.

## Scheduling and run lifecycle

Schedules may be manual, hourly, daily, or weekly. Daily and weekly schedules use the configured IANA timezone; invalid timezone names fall back to UTC during evaluation. The scheduler checks for due work every 30 seconds and records a slot key so the same scheduled occurrence is not run twice.

Only one run for a given check may execute at a time. A run has a configurable reconciliation timeout that bounds the whole agent invocation. If the service restarts during a run, Reconsile marks that run and its check as failed with an interruption log. Inbound WhatsApp commands can request cancellation of an active run.

The agent sends notifications only when a completed reconciliation contains exceptions, and reports any notification failure through `notificationError`; a failed notification does not discard the reconciliation result.

## Notifications

Notifications are sent by the ZeroClaw agent with `zeroclaw channel send`, using the notification type as the channel id and the configured recipient. Configure a matching channel in ZeroClaw for every notification type you use — for example `[channels.email.<alias>]`, `[channels.telegram.<alias>]`, `[channels.discord.<alias>]`, `[channels.whatsapp.<alias>]`, or a custom channel. The channel id passed to `channel send` is the notification type.

Setup guides for custom senders and bots are served from `/docs/` and stored in `web/docs/`.

## Inbound WhatsApp control

The callback is:

```text
https://your-domain.example/api/webhooks/whatsapp
```

Configure `WHATSAPP_WEBHOOK_VERIFY_TOKEN`, `WHATSAPP_APP_SECRET`, `WHATSAPP_BOT_TOKEN`, and `WHATSAPP_PHONE_NUMBER_ID`. In Meta's developer dashboard, subscribe the WhatsApp Business Account to `messages`.

Inbound POST bodies must have a valid `X-Hub-Signature-256` HMAC-SHA256 signature. Events are deduplicated on disk and recorded in run history. By default, all senders are accepted; set `WHATSAPP_ALLOWED_SENDERS` to a comma-separated allowlist for production.

The classifier is intentionally constrained. It can create a check, edit its name/description/statement/schedule, run a check, stop a running check, ask for missing details, or provide help. Requests for credentials, filesystem access, destructive operations, or arbitrary commands are rejected before the model is called. Check source credentials are excluded from classifier context.

## Solana skills

Project-level ZeroClaw skills live under `skills/`. Each skill instructs the agent to query its provider directly with the `http_request` tool:

| Skill | Provider | Purpose | Credential |
| --- | --- | --- | --- |
| `getWalletHoldings` | Solana JSON-RPC | SOL, SPL Token, and Token-2022 balances | Optional `SOLANA_RPC_URL` |
| `getMarketData` | Jupiter | Price/market facts for 1–50 mints | Optional `JUPITER_API_KEY` |
| `getTokenMetadata` | Jupiter | Mint name, symbol, decimals, tags, and metadata | Optional `JUPITER_API_KEY` |
| `getLiquidity` | Birdeye | Token exit-liquidity facts | `BIRDEYE_API_KEY` |
| `getProtocolEvents` | Helius | Recent enhanced transactions/events | `HELIUS_API_KEY` |

Provider credentials are read from the environment by the agent. Because Reconsile launches ZeroClaw as a subprocess, variables set for Reconsile (including those loaded from `.env`) are inherited by the agent.

## HTTP API

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/api/health` | Runtime health |
| `GET` | `/api/state` | Full workspace state |
| `GET` | `/api/auth/status` | Whether password access is required |
| `POST` | `/api/auth/login` | Sign in with the workspace password |
| `POST` | `/api/auth/logout` | End the current session |
| `POST` | `/api/settings` | Update username, workspace, and access mode |
| `POST` | `/api/checks` | Create or replace a check |
| `DELETE` | `/api/checks/{id}` | Delete a check and its runs/exceptions |
| `POST` | `/api/checks/{id}/run` | Run a check |
| `DELETE` | `/api/runs/{id}` | Delete one run-history entry |
| `DELETE` | `/api/exceptions/{id}` | Delete one exception |
| `POST` | `/api/sources/test` | Fetch and preview a source |
| `POST` | `/api/connections` | Save a notification connection |
| `GET`, `POST` | `/api/webhooks/whatsapp` | Verify and receive Meta webhooks |

## Access control

Reconsile starts in **open** mode: the workspace and API are accessible without a password. Open the profile menu (the `•••` button in the sidebar) and choose *Edit profile* to rename the user or workspace agent, or switch access to **Requires credentials**.

When credentials are required, Reconsile protects every workspace and data endpoint behind a password and a signed, HttpOnly session cookie. A password is stored only as a SHA-256 hash in the state file and is never returned to the browser. The login page, `/api/health`, `/api/auth/*`, and the WhatsApp webhook remain reachable without a session so the SPA can render its login screen and machine-to-machine integrations keep working. Reconsile has no tenant isolation, so keep it behind a trusted network or a reverse proxy for anything beyond a single trusted user.

## Shared environment (secrets in ZeroClaw's config)

Secret values are stored in a ZeroClaw config file — encrypted at rest (`enc2:<hex>` ChaCha20-Poly1305) under the keystore key in the adjacent `.secret_key` — rather than in plaintext. Two stores are available:

- **Your machine's ZeroClaw config** (`~/.zeroclaw/`) — the default for `store`, `materialize`, and `status`.
- **A shared copy in the repository** (`.deploy/zeroclaw-secrets/`) — used with `--builtin`, so teammates can materialize a working `.env` right after cloning, without creating API keys from scratch.

```bash
bash scripts/env-secrets.sh store                        # .env -> local ~/.zeroclaw config
bash scripts/env-secrets.sh store WHATSAPP_BOT_TOKEN     # or one key at a time
bash scripts/env-secrets.sh store --builtin              # write into the repo's shared copy instead
bash scripts/env-secrets.sh materialize --builtin        # fresh clone: repo copy -> .env
bash scripts/env-secrets.sh status --builtin             # inspect the shared copy
bash scripts/env-secrets.sh sync-builtin                 # refresh repo copy from your local config
```

`store` shells out to `zeroclaw config set --no-interactive <path> <value>` for each key. Every key maps to a real ZeroClaw config field: WhatsApp, Telegram, and Discord bot tokens use the matching `channels.*` config (and double as real channel credentials), while keys without a natural home use a generic secret slot — `providers.models.openai.<key>.api_key` — which accepts arbitrary aliases. The mapping lives at the top of `scripts/env-secrets.sh`; keys missing from it are auto-mapped to the generic slot.

The mechanism is the same for every key (each one goes through `zeroclaw config set` and is encrypted with the same keystore), but the *location* inside ZeroClaw's config differs. That is because `zeroclaw config set` does not accept arbitrary keys — it only accepts dotted paths that exist in ZeroClaw's config schema, which has no free-form "any key" store. Each `.env` key therefore lands on a real field, chosen by best fit:

**1. Keys with a natural home → the matching channel config.**

| `.env` key | ZeroClaw path |
| --- | --- |
| `WHATSAPP_BOT_TOKEN` | `channels.whatsapp.reconsile.access_token` |
| `WHATSAPP_APP_SECRET` | `channels.whatsapp.reconsile.app_secret` |
| `TELEGRAM_BOT_TOKEN` | `channels.telegram.reconsile.bot_token` |
| `DISCORD_BOT_TOKEN` | `channels.discord.reconsile.bot_token` |

These values *are* channel credentials, and ZeroClaw's schema has exactly those secret fields, so they land in the semantically correct place — and double as real channel credentials if you ever configure ZeroClaw's own WhatsApp, Telegram, or Discord channels.

**2. Keys with no natural home → a generic secret slot.** `BREVO_API_KEY`, `HELIUS_API_KEY`, `BIRDEYE_API_KEY`, and `WHATSAPP_WEBHOOK_VERIFY_TOKEN` are stored at `providers.models.openai.<key>.api_key`. ZeroClaw's schema has no Birdeye, Helius, Brevo, or webhook-verify concept, so no matching field exists. The generic slot works because provider-model aliases are the one place the schema accepts arbitrary names, and `api_key` is a real secret field that is encrypted at rest. The aliases are storage addresses only — nothing references them, so they are inert.

**3. One true exception to "encrypted":** `WHATSAPP_PHONE_NUMBER_ID` is stored at `channels.whatsapp.reconsile.phone_number_id`, a plaintext field in ZeroClaw's schema (not marked as a secret), so it is stored unencrypted. It is an identifier rather than a credential; if you would rather have it encrypted like the rest, point it at the generic slot in the `SECRETS` table — a one-line change.

`materialize` starts from `.env.example` (or the current `.env`), decrypts each stored value with ZeroClaw's keystore key, and writes `.env`. Because the `zeroclaw` CLI never prints stored secrets, decryption reads `config.toml` directly — the same `enc2:<hex>` ChaCha20-Poly1305 format ZeroClaw itself uses. That decrypt is the one step that calls `python3` (a short heredoc); everything else is bash.

The `zeroclaw` binary comes from `ZEROCLAW_BIN` (default `zeroclaw`); the local config directory is taken from `--config-dir`, `ZEROCLAW_CONFIG_DIR`, or `~/.zeroclaw`.

**Team workflow:** keep `.deploy/zeroclaw-secrets/` current by running `sync-builtin` (or `store --builtin`) after editing `.env`, and commit both files. Teammates then run `bash scripts/env-secrets.sh materialize --builtin` after cloning.

**Security note:** the shared copy ships the keystore key next to the ciphertext, so anyone with read access to the repository can decrypt every secret it contains — including the model provider API key. That is the price of letting teammates run without creating keys; keep the repository private. If the key ever leaks, replace `~/.zeroclaw/.secret_key` (move it aside and let ZeroClaw regenerate it), re-run `store`, then `sync-builtin`.

## Configuration

All supported variables are present in `.env.example`.

| Variable | Default | Purpose |
| --- | --- | --- |
| `HOST` | `127.0.0.1` | Listen address |
| `PORT` | `4173` | Listen port |
| `WEB_ROOT` | `web` | Static frontend directory |
| `RECONSILE_DATA_DIR` | `.data` | State and webhook markers |
| `RUN_TIMEOUT_SECONDS` | `180` | Maximum reconciliation duration |
| `ZEROCLAW_BIN` | `zeroclaw` | ZeroClaw executable |
| `ZEROCLAW_AGENT` | `reconcile` | Reconciliation agent name |
| `WHATSAPP_BOT_TOKEN` | unset | Meta Cloud API token (inbound replies) |
| `WHATSAPP_PHONE_NUMBER_ID` | unset | Meta sender Phone Number ID (inbound replies) |
| `WHATSAPP_APP_SECRET` | unset | Webhook signature verification |
| `WHATSAPP_WEBHOOK_VERIFY_TOKEN` | unset | Meta webhook subscription handshake |
| `WHATSAPP_ALLOWED_SENDERS` | empty (allow all) | Comma-separated inbound sender allowlist |
| `WHATSAPP_ZEROCLAW_AGENT` | `ZEROCLAW_AGENT` | Dedicated inbound command agent |
| `WHATSAPP_AGENT_TIMEOUT_SECONDS` | `180` | Inbound classifier timeout |
| `WHATSAPP_GRAPH_API_VERSION` | `v25.0` | Meta Graph API version |
| `SOLANA_RPC_URL` | Solana mainnet public RPC | Wallet holdings provider (read by the agent) |
| `JUPITER_API_URL` | `https://api.jup.ag` | Jupiter base URL (read by the agent) |
| `JUPITER_API_KEY` | unset | Optional Jupiter API key |
| `BIRDEYE_API_URL` | `https://public-api.birdeye.so` | Birdeye base URL (read by the agent) |
| `BIRDEYE_API_KEY` | unset | Required for liquidity |
| `HELIUS_API_URL` | `https://api-mainnet.helius-rpc.com` | Helius base URL (read by the agent) |
| `HELIUS_API_KEY` | unset | Required for protocol events |

## Persistence and security

State is written atomically through a temporary file and rename. Back up the entire `RECONSILE_DATA_DIR`, not only `state.json`, if you need run history and webhook deduplication to move together.

The state file can include source bearer tokens, custom headers, and notification credentials. Keep the data directory outside the public web root, restrict its filesystem permissions, exclude it from backups that are not encrypted, and never commit it. The repository's `.gitignore` excludes `.env`, `.data/`, Rust build output, and log files.

HTTP source URLs can reach any address accessible to the ZeroClaw agent. Treat check creation as a privileged operation and apply network egress controls when Reconsile is exposed to untrusted users.

## Build, test, and release

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo build --release --manifest-path src-tauri/Cargo.toml
```

The release executable is `src-tauri/target/release/reconsile`.

## Production deployment

The included `.deploy/reconsile.online.nginx` serves the static frontend and proxies `/api/` to `127.0.0.1:4173`. Its domain, certificate paths, web root, and timeouts are deployment-specific examples and must be adapted for your host.

A typical deployment is:

1. Build the release binary.
2. Copy `web/` to the nginx document root.
3. Copy `.env.example` to a protected environment file and provide production secrets outside Git.
4. Run the binary as a persistent system service with a durable `RECONSILE_DATA_DIR`.
5. Enable workspace credentials (or place authentication in front of both the UI and `/api/`).
6. Configure TLS and proxy `/api/` with timeouts longer than `RUN_TIMEOUT_SECONDS`.
7. Back up the data directory and monitor failed/interrupted runs.

The example nginx configuration uses a 960-second proxy timeout because production agent runs may be substantially longer than the local defaults.

## Repository layout

```text
.
├── .deploy/          # nginx and ZeroClaw deployment examples + shared zeroclaw-secrets/
├── public/           # standalone public/static assets
├── skills/           # project ZeroClaw Solana skills
├── src-tauri/        # Rust service, tests, lockfile, and legacy icon assets
└── web/              # production SPA and setup guides served by Rust
```
