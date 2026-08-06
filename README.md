# Reconsile
<img width="1942" height="809" alt="ChatGPT Image Aug 6, 2026, 09_58_48 PM" src="https://github.com/user-attachments/assets/69590a36-929c-4d26-a575-5e8d37dac67a" />

Reconsile is a self-hosted reconciliation workspace for comparing operational records, tracking exceptions, and notifying the people who need to act. It combines a responsive browser UI with a Rust/Axum service, durable JSON state, a built-in scheduler, optional ZeroClaw analysis, inbound WhatsApp control, and Solana data tools.

The repository contains a web application, not a desktop runtime. Tauri, Slint, a webview, Node.js, and a graphical display server are not required to run it.

## What it does

- Create reconciliation checks from a plain-language statement, one or more data sources, an optional Solana wallet, a schedule, and notification rules.
- Load sources from HTTP(S) endpoints with no authentication, bearer authentication, or a custom header. Built-in `demo://` sources make the initial workspace usable offline.
- Run checks manually or on hourly, daily, and weekly schedules in an IANA timezone.
- Use deterministic demo reconciliation locally, or delegate the comparison to a configured ZeroClaw agent.
- Track run status, record counts, match rates, exceptions, detailed run logs, and notification outcomes.
- Persist checks, connections, run history, exceptions, webhook markers, and market snapshots under one data directory.
- Notify by hosted email (Brevo), Telegram, Discord DM, WhatsApp Cloud API, or a custom ZeroClaw channel.
- Accept signed WhatsApp webhook messages that can create, edit, run, or stop checks.
- Expose five Solana tools for wallet holdings, market data, token metadata, liquidity, and protocol events.

The UI includes overview, checks, run history, and exception views; workspace search; in-app activity notifications; check and schedule editors; connection setup; responsive mobile navigation; and run-log inspection.

## Architecture

```text
Browser
  └── prebuilt React workspace (web/)
       └── Rust HTTP service (Axum)
            ├── workspace CRUD and run APIs
            ├── source fetcher and reconciliation engine
            ├── 30-second schedule evaluator
            ├── atomic JSON persistence
            ├── outbound notification adapters
            ├── signed WhatsApp webhook controller
            └── Solana provider endpoints
                 ├── Solana JSON-RPC
                 ├── Jupiter
                 ├── Birdeye
                 └── Helius

Optional ZeroClaw process
  ├── reconciliation agent
  ├── WhatsApp command classifier
  ├── custom notification channels
  └── project skills under skills/
```

The Rust service serves both `/api/*` and the static SPA. Unknown browser routes fall back to `web/index.html`.

## Requirements

- A current stable Rust toolchain with Cargo.
- Network access for HTTP data sources and any configured notification or Solana providers.
- ZeroClaw only when AI reconciliation, WhatsApp commands, or custom ZeroClaw channels are enabled.

No database or JavaScript build tool is required for the checked-in application. The compiled frontend assets are already in `web/`.

## Quick start

From the repository root:

```bash
cp .env.example .env
cargo run --manifest-path src-tauri/Cargo.toml
```

Open <http://127.0.0.1:4173>. The service loads `.env` without overriding variables already exported by the process.

On first start, Reconsile creates `.data/state.json` with demo checks and sample history. The demo sources and deterministic engine work without third-party credentials. Change `RECONSILE_DATA_DIR` if the state should live elsewhere.

Check service health with:

```bash
curl http://127.0.0.1:4173/api/health
```

The response reports `"zeroclaw":"demo"` until `ZEROCLAW_ENABLED=true`.

## Reconciliation modes

### Demo mode

Demo mode is the default. It supports the bundled `demo://orders`, `demo://payouts`, and `demo://inventory` sources. The deterministic comparison contains a purpose-built Stripe settlement example; other checks return basic record totals unless ZeroClaw is enabled.

### ZeroClaw mode

Set `ZEROCLAW_ENABLED=true` after installing and configuring ZeroClaw. For each run, Reconsile:

1. Fetches every configured source.
2. Builds a prompt from the check statement, optional wallet address, and source data.
3. Invokes `zeroclaw agent -a <agent> -m <prompt>`.
4. Normalizes the returned JSON into a summary, counts, and exceptions.
5. Persists the result and sends enabled notifications when exceptions exist.

The agent must return JSON containing `summary`, `records`, `matched`, and `exceptions`. Each exception may contain `title`, `detail`, `amount`, and `severity`.

The included `.deploy/zeroclaw.config.toml` defines a `reconcile` agent and loads the project skill bundle. Adjust its provider and risk settings for your environment before production use.

## Data sources

A check can use multiple sources. Supported URLs are:

- `https://...` and `http://...` endpoints.
- `demo://orders`, `demo://payouts`, and `demo://inventory` for local evaluation.

HTTP sources are fetched with `GET`. Authentication can be disabled, sent as `Authorization: Bearer <token>`, or supplied through a custom header. The source tester reports a record count and a small preview before a check is saved.

JSON responses are passed to the reconciliation engine as structured data. Non-JSON responses, including CSV text, are retained as bounded text for ZeroClaw analysis. Source credentials are persisted in the workspace state file, so the data directory must be protected like a secrets store.

## Scheduling and run lifecycle

Schedules may be manual, hourly, daily, or weekly. Daily and weekly schedules use the configured IANA timezone; invalid timezone names fall back to UTC during evaluation. The scheduler checks for due work every 30 seconds and records a slot key so the same scheduled occurrence is not run twice.

Only one run for a given check may execute at a time. A run has a configurable reconciliation timeout and a separate notification timeout. If the service restarts during a run, Reconsile marks that run and its check as failed with an interruption log. Inbound WhatsApp commands can request cancellation of an active run.

Notifications are sent only when a completed reconciliation contains exceptions. A notification failure is saved and returned separately; it does not discard the reconciliation result.

## Notifications

| Channel | Hosted configuration | Custom configuration |
| --- | --- | --- |
| Email | `BREVO_API_KEY`; sends from `notifications@reconsile.online` | Routed through the configured ZeroClaw channel |
| Telegram | `TELEGRAM_BOT_TOKEN` | Bot token saved through Connections or on the check |
| Discord | `DISCORD_BOT_TOKEN` | Bot token saved through Connections or on the check |
| WhatsApp | `WHATSAPP_BOT_TOKEN` and `WHATSAPP_PHONE_NUMBER_ID` | Cloud API token and Phone Number ID saved through Connections or on the check |
| Other | — | `zeroclaw channel send` using the notification type as the channel ID |

Telegram accepts a numeric chat ID or a username that has already messaged the bot. Discord expects a numeric user ID and creates/caches a private DM channel. WhatsApp accepts a recipient phone number; when `WHATSAPP_TEMPLATE_NAME` is set, outbound alerts use that approved template instead of a free-form text message.

Setup guides for custom senders and bots are served from `/docs/` and stored in `web/docs/`.

## Inbound WhatsApp control

The callback is:

```text
https://your-domain.example/api/webhooks/whatsapp
```

Configure `WHATSAPP_WEBHOOK_VERIFY_TOKEN`, `WHATSAPP_APP_SECRET`, `WHATSAPP_BOT_TOKEN`, `WHATSAPP_PHONE_NUMBER_ID`, and `ZEROCLAW_ENABLED=true`. In Meta's developer dashboard, subscribe the WhatsApp Business Account to `messages`.

Inbound POST bodies must have a valid `X-Hub-Signature-256` HMAC-SHA256 signature. Events are deduplicated on disk and recorded in run history. By default, all senders are accepted; set `WHATSAPP_ALLOWED_SENDERS` to a comma-separated allowlist for production.

The classifier is intentionally constrained. It can create a check, edit its name/description/statement/schedule, run a check, stop a running check, ask for missing details, or provide help. Requests for credentials, filesystem access, destructive operations, or arbitrary commands are rejected before the model is called. Check source credentials are excluded from classifier context.

## Solana skills and API

Project-level ZeroClaw skills live under `skills/` and call these backend endpoints:

| Endpoint | Provider | Purpose | Credential |
| --- | --- | --- | --- |
| `POST /api/skills/getWalletHoldings` | Solana JSON-RPC | SOL, SPL Token, and Token-2022 balances | Optional custom RPC URL |
| `POST /api/skills/getMarketData` | Jupiter | Price/market facts for 1–50 mints | Optional API key |
| `POST /api/skills/getTokenMetadata` | Jupiter | Mint name, symbol, decimals, tags, and metadata | Optional API key |
| `POST /api/skills/getLiquidity` | Birdeye | Token exit-liquidity facts | `BIRDEYE_API_KEY` |
| `POST /api/skills/getProtocolEvents` | Helius | Recent enhanced transactions/events | `HELIUS_API_KEY` |

Addresses are validated as base58 Solana addresses before provider calls. Market data maintains an atomic rolling snapshot per mint under `.data/skill-snapshots/`, returning the previous and current observations plus changes in price, liquidity, 24-hour price change, volume, and volatility when those fields are available.

Example:

```bash
curl -sS http://127.0.0.1:4173/api/skills/getWalletHoldings \
  -H 'content-type: application/json' \
  -d '{"wallet_address":"<SOLANA_WALLET_ADDRESS>"}'
```

## HTTP API

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/api/health` | Runtime health and reconciliation mode |
| `GET` | `/api/state` | Full workspace state |
| `POST` | `/api/checks` | Create or replace a check |
| `DELETE` | `/api/checks/{id}` | Delete a check and its runs/exceptions |
| `POST` | `/api/checks/{id}/run` | Run a check |
| `DELETE` | `/api/runs/{id}` | Delete one run-history entry |
| `DELETE` | `/api/exceptions/{id}` | Delete one exception |
| `POST` | `/api/sources/test` | Fetch and preview a source |
| `POST` | `/api/connections` | Save a notification connection |
| `GET`, `POST` | `/api/webhooks/whatsapp` | Verify and receive Meta webhooks |

The five Solana routes listed above are part of the same unauthenticated HTTP service. Reconsile currently has no built-in user authentication or tenant isolation; put it behind an authenticated reverse proxy or another access-control layer before exposing it beyond a trusted environment.

## Configuration

All supported variables are present in `.env.example`.

| Variable | Default | Purpose |
| --- | --- | --- |
| `HOST` | `127.0.0.1` | Listen address |
| `PORT` | `4173` | Listen port |
| `WEB_ROOT` | `web` | Static frontend directory |
| `RECONSILE_DATA_DIR` | `.data` | State, webhook markers, and skill snapshots |
| `RUN_TIMEOUT_SECONDS` | `180` | Maximum reconciliation duration |
| `NOTIFICATION_TIMEOUT_SECONDS` | `30` | Maximum notification phase duration |
| `ZEROCLAW_ENABLED` | `false` | Enable ZeroClaw reconciliation and commands |
| `ZEROCLAW_BIN` | `zeroclaw` | ZeroClaw executable |
| `ZEROCLAW_AGENT` | `reconcile` | Reconciliation agent name |
| `RECONSILE_API_BASE_URL` | `http://127.0.0.1:4173` | Base URL used by project skills |
| `BREVO_API_KEY` | unset | Hosted email delivery |
| `TELEGRAM_BOT_TOKEN` | unset | Hosted Telegram bot |
| `DISCORD_BOT_TOKEN` | unset | Hosted Discord bot |
| `WHATSAPP_BOT_TOKEN` | unset | Meta Cloud API token |
| `WHATSAPP_PHONE_NUMBER_ID` | unset | Meta sender Phone Number ID |
| `WHATSAPP_APP_SECRET` | unset | Webhook signature verification |
| `WHATSAPP_WEBHOOK_VERIFY_TOKEN` | unset | Meta webhook subscription handshake |
| `WHATSAPP_ALLOWED_SENDERS` | empty (allow all) | Comma-separated inbound sender allowlist |
| `WHATSAPP_ZEROCLAW_AGENT` | `ZEROCLAW_AGENT` | Dedicated inbound command agent |
| `WHATSAPP_AGENT_TIMEOUT_SECONDS` | `180` | Inbound classifier timeout |
| `WHATSAPP_GRAPH_API_VERSION` | `v25.0` | Meta Graph API version |
| `WHATSAPP_TEMPLATE_NAME` | unset | Optional outbound template |
| `WHATSAPP_TEMPLATE_LANGUAGE` | `en_US` | Outbound template language |
| `SOLANA_RPC_URL` | Solana mainnet public RPC | Wallet holdings provider |
| `JUPITER_API_URL` | `https://api.jup.ag` | Jupiter base URL |
| `JUPITER_API_KEY` | unset | Optional Jupiter API key |
| `BIRDEYE_API_URL` | `https://public-api.birdeye.so` | Birdeye base URL |
| `BIRDEYE_API_KEY` | unset | Required for liquidity |
| `HELIUS_API_URL` | `https://api-mainnet.helius-rpc.com` | Helius base URL |
| `HELIUS_API_KEY` | unset | Required for protocol events |

## Persistence and security

State is written atomically through a temporary file and rename. Back up the entire `RECONSILE_DATA_DIR`, not only `state.json`, if you need run history, webhook deduplication, and market baselines to move together.

The state file can include source bearer tokens, custom headers, and notification credentials. Keep the data directory outside the public web root, restrict its filesystem permissions, exclude it from backups that are not encrypted, and never commit it. The repository's `.gitignore` excludes `.env`, `.data/`, Rust build output, and log files.

HTTP source URLs can reach any address accessible to the service. Treat check creation as a privileged operation and apply network egress controls when Reconsile is exposed to untrusted users.

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
5. Place authentication in front of both the UI and `/api/`.
6. Configure TLS and proxy `/api/` with timeouts longer than `RUN_TIMEOUT_SECONDS` plus `NOTIFICATION_TIMEOUT_SECONDS`.
7. Back up the data directory and monitor failed/interrupted runs.

The example nginx configuration uses a 960-second proxy timeout because production agent runs may be substantially longer than the local defaults.

## Repository layout

```text
.
├── .deploy/          # nginx and ZeroClaw deployment examples
├── public/           # standalone public/static assets
├── skills/           # project ZeroClaw Solana skills
├── src-tauri/        # Rust service, tests, lockfile, and legacy icon assets
└── web/              # production SPA and setup guides served by Rust
```
