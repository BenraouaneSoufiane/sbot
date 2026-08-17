#!/usr/bin/env bash
# env-secrets.sh — store each `.env` secret in ZeroClaw's config via the zeroclaw
# CLI, and rebuild `.env` from the stored secrets.
#
# Each secret key from `.env` is written into a ZeroClaw config file with
#
#     zeroclaw config set --no-interactive <dotted.path> <value>
#
# ZeroClaw stores secret fields encrypted at rest (`enc2:<hex>` ChaCha20-Poly1305
# under the adjacent `.secret_key`), so the plaintext values live only in your
# local `.env` and the keystore-protected config. Two stores are supported:
#
#   * the local machine's ZeroClaw config (default: ~/.zeroclaw), and
#   * a repo copy at .deploy/zeroclaw-secrets/ (use `--builtin`), so teammates
#     can run `materialize --builtin` after a fresh clone without creating API
#     keys from scratch. That copy intentionally ships the keystore key next to
#     the ciphertext, so keep the repository private.
#
# Commands:
#   store [KEY ...]       Push secret values from `.env` into the config.
#                         With no KEY, every secret present in `.env` is stored.
#   materialize           Rebuild `.env` from `.env.example` (or the current
#                         `.env`) with secret values decrypted from the config.
#   status                Show which secrets are stored and where.
#   sync-builtin          Copy your local ZeroClaw config + keystore key into
#                         .deploy/zeroclaw-secrets/ (then commit to share).
#
# Options: --builtin (use .deploy/zeroclaw-secrets/ instead of ~/.zeroclaw),
#          --config-dir DIR, --env FILE (default: ./.env),
#          --template FILE (default: ./.env.example)
#
# Every key maps to a real ZeroClaw config path (see SECRETS below). Bot tokens
# use ZeroClaw's own channel config; keys without a natural home use a generic
# secret slot, providers.models.openai.<key_lower>.api_key (aliases there accept
# any lowercase name). The only step that needs Python is decrypting an `enc2:`
# blob during materialize — openssl's `enc` does not support AEAD ciphers, and
# the zeroclaw CLI never prints stored secrets. Requires python3 + the
# `cryptography` package for that single step.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="$ROOT/.env"
TEMPLATE_FILE="$ROOT/.env.example"
BUILTIN_DIR="$ROOT/.deploy/zeroclaw-secrets"
LIVE_DIR="${ZEROCLAW_CONFIG_DIR:-${HOME}/.zeroclaw}"
if [[ ! -d "$LIVE_DIR" && -n "${XDG_CONFIG_HOME:-}" && -d "$XDG_CONFIG_HOME/zeroclaw" ]]; then
  LIVE_DIR="$XDG_CONFIG_HOME/zeroclaw"
fi
CONFIG_DIR="$LIVE_DIR"
ZEROCLAW_BIN="${ZEROCLAW_BIN:-zeroclaw}"

# .env secret key -> ZeroClaw config path (dotted). Keys not listed here are
# auto-mapped to providers.models.openai.<key_lower>.api_key during store, but
# only keys in this table are materialized back into .env.
SECRETS=(
  "WHATSAPP_BOT_TOKEN channels.whatsapp.sbot.access_token"
  "WHATSAPP_APP_SECRET channels.whatsapp.sbot.app_secret"
  "WHATSAPP_PHONE_NUMBER_ID channels.whatsapp.sbot.phone_number_id"
  "WHATSAPP_WEBHOOK_VERIFY_TOKEN channels.whatsapp.sbot.verify_token"
  "TELEGRAM_BOT_TOKEN channels.telegram.sbot.bot_token"
  "DISCORD_BOT_TOKEN channels.discord.sbot.bot_token"
  "BREVO_API_KEY providers.models.openai.brevo_api_key.api_key"
  "HELIUS_API_KEY providers.models.openai.helius_api_key.api_key"
  "BIRDEYE_API_KEY providers.models.openai.birdeye_api_key.api_key"
)

SECRET_RE='(TOKEN|SECRET|KEY|PASSWORD|PHONE_NUMBER_ID)'

die() { echo "error: $*" >&2; exit 1; }

usage() {
  awk '/^set -euo pipefail$/ { exit } NR > 1 { print }' "$0" | sed 's/^# \{0,1\}//'
}

in_secrets() { # key -> 0 if listed in SECRETS
  local entry
  for entry in "${SECRETS[@]}"; do
    [[ "$entry" == "$1 "* ]] && return 0
  done
  return 1
}

path_for() { # key -> dotted path (generic fallback for unknown keys)
  local key="$1" entry low
  for entry in "${SECRETS[@]}"; do
    if [[ "$entry" == "$key "* ]]; then
      echo "${entry#* }"
      return
    fi
  done
  low="$(printf '%s' "$key" | tr '[:upper:]' '[:lower:]')"
  echo "providers.models.openai.${low}.api_key"
}

env_value() { # key -> value from ENV_FILE ('' if absent)
  local key="$1" line
  while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line%$'\r'}"
    if [[ "$line" == "$key="* ]]; then
      printf '%s' "${line#*=}"
      return
    fi
  done < "$ENV_FILE"
}

zeroclaw_set() { # dotted value
  local dotted="$1" value="$2"
  local args=(config set --no-interactive "$dotted")
  if [[ "$value" == -* ]]; then
    args+=(-- "$value")
  else
    args+=("$value")
  fi
  "$ZEROCLAW_BIN" --config-dir "$CONFIG_DIR" "${args[@]}" >/dev/null
}

lookup_value() { # dotted -> raw TOML string value (quotes stripped) or ''
  local dotted="$1" section field
  section="${dotted%.*}"
  field="${dotted##*.}"
  [[ "$section" == *.* ]] || return 0
  awk -v sec="[$section]" -v fld="$field" '
    $0 == sec { insec = 1; next }
    /^\[/ { if (insec) exit }
    insec && index($0, fld " = ") == 1 {
      line = $0
      sub(/^[^=]*= *"/, "", line)
      sub(/"[[:space:]]*$/, "", line)
      print line
      exit
    }
  ' "$CONFIG_DIR/config.toml" 2>/dev/null || true
}

stored_value() { # key -> decrypted value ('' if not stored)
  local key="$1" dotted raw blob
  dotted="$(path_for "$key")"
  raw="$(lookup_value "$dotted")"
  [[ -n "$raw" ]] || { echo ""; return; }
  if [[ "$raw" == enc2:* ]]; then
    blob="${raw#enc2:}"
    python3 - "$CONFIG_DIR/.secret_key" "$blob" <<'PY'
import sys
from pathlib import Path
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
key = bytes.fromhex(Path(sys.argv[1]).read_text().strip())
blob = bytes.fromhex(sys.argv[2])
print(ChaCha20Poly1305(key).decrypt(blob[:12], blob[12:], None).decode())
PY
  else
    printf '%s' "$raw"
  fi
}

cmd_store() {
  local -a selected=()
  [[ -f "$ENV_FILE" ]] || die "no $ENV_FILE"
  local line key value dotted stored=0 skipped=0 failed=0

  if [[ $# -gt 0 ]]; then
    selected=("$@")
  else
    while IFS= read -r line || [[ -n "$line" ]]; do
      line="${line%$'\r'}"
      if [[ "$line" =~ ^([A-Za-z_][A-Za-z0-9_]*)=(.*)$ ]]; then
        key="${BASH_REMATCH[1]}"
        value="${BASH_REMATCH[2]}"
        if [[ -n "$value" && "$key" =~ $SECRET_RE ]]; then
          selected+=("$key")
        fi
      fi
    done < "$ENV_FILE"
  fi

  for key in "${selected[@]}"; do
    value="$(env_value "$key")"
    if [[ -z "$value" ]]; then
      echo "  $key: skipped (empty in $ENV_FILE)"
      skipped=$((skipped + 1))
      continue
    fi
    dotted="$(path_for "$key")"
    if zeroclaw_set "$dotted" "$value"; then
      echo "  $key: stored -> $dotted"
      if ! in_secrets "$key"; then
        echo "  $key: auto-mapped (add it to SECRETS to pin the path)"
      fi
      stored=$((stored + 1))
    else
      echo "  $key: FAILED"
      failed=$((failed + 1))
    fi
  done
  echo "stored $stored secret(s) in $CONFIG_DIR/config.toml"
  [[ $failed -eq 0 ]] || die "$failed secret(s) could not be stored"
}

cmd_materialize() {
  local src="$ENV_FILE"
  [[ -f "$src" ]] || src="$TEMPLATE_FILE"
  [[ -f "$src" ]] || die "neither $ENV_FILE nor $TEMPLATE_FILE exists"
  [[ -f "$CONFIG_DIR/.secret_key" ]] || die "keystore key not found at $CONFIG_DIR/.secret_key"

  local out="" line key handled=""
  while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line%$'\r'}"
    if [[ "$line" =~ ^([A-Za-z_][A-Za-z0-9_]*)=(.*)$ ]] && in_secrets "${BASH_REMATCH[1]}"; then
      key="${BASH_REMATCH[1]}"
      out+="${key}=$(stored_value "$key")"$'\n'
      handled+="$key "
    else
      out+="$line"$'\n'
    fi
  done < "$src"

  local entry
  for entry in "${SECRETS[@]}"; do
    key="${entry%% *}"
    case " $handled " in
      *" $key "*) ;;
      *) out+="${key}=$(stored_value "$key")"$'\n' ;;
    esac
  done

  printf '%s' "$out" > "$ENV_FILE"
  echo "materialized into $ENV_FILE from $CONFIG_DIR/config.toml"
}

cmd_status() {
  local entry key dotted raw
  for entry in "${SECRETS[@]}"; do
    key="${entry%% *}"
    dotted="${entry#* }"
    raw="$(lookup_value "$dotted")"
    if [[ -n "$raw" ]]; then
      printf '  %-34s stored  %s\n' "$key" "$dotted"
    else
      printf '  %-34s empty   %s\n' "$key" "$dotted"
    fi
  done
}

cmd_sync_builtin() {
  local src_toml="$LIVE_DIR/config.toml" src_key="$LIVE_DIR/.secret_key"
  [[ -f "$src_toml" && -f "$src_key" ]] || die "no config.toml/.secret_key in $LIVE_DIR — run \`bash scripts/env-secrets.sh store\` first"
  mkdir -p "$BUILTIN_DIR"
  cp "$src_toml" "$BUILTIN_DIR/config.toml"
  cp "$src_key" "$BUILTIN_DIR/.secret_key"
  chmod 600 "$BUILTIN_DIR/.secret_key"
  echo "copied ZeroClaw config + keystore key into $BUILTIN_DIR (commit both files to share)"
}

main() {
  local cmd="" args=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
      store | materialize | status | sync-builtin) cmd="$1" ;;
      --builtin) CONFIG_DIR="$BUILTIN_DIR" ;;
      --config-dir) [[ $# -ge 2 ]] || die "--config-dir needs a value"; CONFIG_DIR="$2"; shift ;;
      --env) [[ $# -ge 2 ]] || die "--env needs a value"; ENV_FILE="$2"; shift ;;
      --template) [[ $# -ge 2 ]] || die "--template needs a value"; TEMPLATE_FILE="$2"; shift ;;
      -h | --help) usage; exit 0 ;;
      *)
        if [[ -z "$cmd" ]]; then
          die "unknown argument: $1"
        fi
        args+=("$1")
        ;;
    esac
    shift
  done
  [[ -n "$cmd" ]] || die "missing command (store | materialize | status | sync-builtin)"

  case "$cmd" in
    store) cmd_store "${args[@]}" ;;
    materialize) cmd_materialize ;;
    status) cmd_status ;;
    sync-builtin) cmd_sync_builtin ;;
  esac
}

main "$@"
