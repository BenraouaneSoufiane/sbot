# Reconsile

Reconsile is a browser application served by a Rust HTTP backend. The server hosts the web interface, persists workspace state, runs reconciliation checks and schedules, and sends configured notifications.

There is no desktop runtime: Tauri, Slint, a webview, and a graphical display server are not required.

## Run locally

From the repository root:

```bash
cargo run --manifest-path src-tauri/Cargo.toml
```

Then open <http://127.0.0.1:4173>. The default configuration serves `web/` and stores state in `.data/state.json`.

Useful environment variables are listed in `.env.example`. For example:

```bash
HOST=0.0.0.0 PORT=4173 RECONSILE_DATA_DIR=.data \
  cargo run --manifest-path src-tauri/Cargo.toml
```

Build and verify with:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo build --release --manifest-path src-tauri/Cargo.toml
```

The release executable is `src-tauri/target/release/reconsile`.

## Production deployment

The included nginx configuration serves the web assets and proxies `/api/` to the Rust service on `127.0.0.1:4173`. A production deployment therefore consists of:

1. Build the release binary.
2. Copy `web/` to `/var/www/reconsile.online/`.
3. Run the Rust binary as a persistent system service with `PORT=4173` and a durable `RECONSILE_DATA_DIR`.
4. Reload nginx if its configuration changed.

## Integrations

Without configuration, checks use a deterministic demo reconciliation engine. Set `ZEROCLAW_ENABLED=true` to enable the configured ZeroClaw agent. Notifications support Brevo email, Telegram, Discord, and ZeroClaw-backed custom channels; their credentials are read from the environment.

### WhatsApp inbound messages

Set `WHATSAPP_APP_SECRET`, `WHATSAPP_WEBHOOK_VERIFY_TOKEN`,
`WHATSAPP_BOT_TOKEN`, `WHATSAPP_PHONE_NUMBER_ID`, and
`ZEROCLAW_ENABLED=true`. Optionally set `WHATSAPP_ALLOWED_SENDERS` to a comma-separated
list of phone numbers that may manage checks (the WhatsApp connection address or
destination is also accepted); when no allowlist is configured, all senders are
accepted. In Meta's developer dashboard, subscribe the WhatsApp
Business Account to `messages` and use this callback URL:

```text
https://reconsile.online/api/webhooks/whatsapp
```

Use this public Privacy Policy URL in the Meta app's Basic settings:

```text
https://reconsile.online/privacy-policy.html
```

Use the same arbitrary verify-token value in Meta and
`WHATSAPP_WEBHOOK_VERIFY_TOKEN`. POST requests are authenticated with the Meta
app secret before text, button, and list replies are classified by ZeroClaw. The
classifier returns a closed set of commands; only create, edit, run, and stop are
executed by the server. It is not given credentials or filesystem tools, and
sensitive, destructive, and filesystem requests are rejected before classification.
Set `WHATSAPP_ZEROCLAW_AGENT` if inbound messages should use a dedicated agent.

Project-level ZeroClaw skills live under `skills/`. The Solana data skills call the Rust endpoints under `/api/skills/`; wallet holdings use Solana RPC, market data and token metadata use Jupiter, liquidity uses Birdeye, and protocol events use Helius. Configure the corresponding URLs and API keys from `.env.example` before using provider-backed skills.

## Architecture

```text
Browser UI (web/)
  └── Rust HTTP server (Axum)
       ├── /api state and reconciliation endpoints
       ├── atomic JSON persistence and scheduler
       ├── authenticated HTTP record-source fetcher
       ├── deterministic or ZeroClaw reconciliation
       └── notification delivery
```
