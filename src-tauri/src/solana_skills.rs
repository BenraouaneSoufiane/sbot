use axum::{http::StatusCode, Json};
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::OnceLock};
use tokio::sync::Mutex;

type ApiError = (StatusCode, Json<Value>);
type ApiResult = Result<Json<Value>, ApiError>;

const DEFAULT_SOLANA_RPC_URL: &str = "https://api.mainnet-beta.solana.com";
const DEFAULT_JUPITER_API_URL: &str = "https://api.jup.ag";
const DEFAULT_BIRDEYE_API_URL: &str = "https://public-api.birdeye.so";
const DEFAULT_HELIUS_API_URL: &str = "https://api-mainnet.helius-rpc.com";

static MARKET_SNAPSHOT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Deserialize)]
pub struct WalletHoldingsRequest {
    wallet_address: String,
}

#[derive(Deserialize)]
pub struct MarketDataRequest {
    token_addresses: Vec<String>,
}

#[derive(Deserialize)]
pub struct TokenRequest {
    token_address: String,
}

#[derive(Deserialize)]
pub struct ProtocolEventsRequest {
    address: String,
    #[serde(default = "default_event_limit")]
    limit: u8,
}

fn default_event_limit() -> u8 {
    20
}

fn error(status: StatusCode, message: impl ToString) -> ApiError {
    (status, Json(json!({ "error": message.to_string() })))
}

fn validate_address(address: &str, field: &str) -> Result<(), ApiError> {
    const BASE58: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    if !(32..=44).contains(&address.len()) || !address.chars().all(|c| BASE58.contains(c)) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            format!("{field} must be a base58 Solana address"),
        ));
    }
    Ok(())
}

fn client() -> Result<Client, ApiError> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn json_response(response: reqwest::Response) -> Result<Value, ApiError> {
    let status = response.status();
    let body = response.json::<Value>().await.map_err(|e| {
        error(
            StatusCode::BAD_GATEWAY,
            format!("Provider returned invalid JSON: {e}"),
        )
    })?;
    if !status.is_success() {
        return Err(error(
            StatusCode::BAD_GATEWAY,
            format!("Provider request failed ({status}): {body}"),
        ));
    }
    Ok(body)
}

fn env_url(name: &str, default: &str) -> String {
    std::env::var(name)
        .unwrap_or_else(|_| default.into())
        .trim_end_matches('/')
        .to_owned()
}

fn market_snapshot_path() -> PathBuf {
    std::env::var_os("RECONSILE_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".data"))
        .join("skill-snapshots")
        .join("get-market-data.json")
}

fn metric_comparison(previous: Option<&Value>, current: Option<&Value>) -> Value {
    const METRICS: [&str; 5] = [
        "usdPrice",
        "liquidity",
        "priceChange24h",
        "volume24h",
        "volatility24h",
    ];
    let metrics = METRICS
        .into_iter()
        .filter_map(|metric| {
            let before = previous?.get(metric)?.as_f64()?;
            let after = current?.get(metric)?.as_f64()?;
            let percent = (before != 0.0).then(|| (after - before) / before * 100.0);
            Some((
                metric.to_owned(),
                json!({
                    "previous": before,
                    "current": after,
                    "absoluteChange": after - before,
                    "percentChange": percent
                }),
            ))
        })
        .collect::<serde_json::Map<_, _>>();
    Value::Object(metrics)
}

async fn snapshot_market_data(
    token_addresses: &[String],
    current: &Value,
) -> Result<Value, ApiError> {
    let _guard = MARKET_SNAPSHOT_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .await;
    let path = market_snapshot_path();
    let mut stored = match tokio::fs::read_to_string(&path).await {
        Ok(raw) => serde_json::from_str::<Value>(&raw).map_err(|e| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Invalid market snapshot store: {e}"),
            )
        })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({"tokens": {}}),
        Err(error_value) => return Err(error(StatusCode::INTERNAL_SERVER_ERROR, error_value)),
    };
    let captured_at = Utc::now().to_rfc3339();
    let stored_tokens = stored
        .get_mut("tokens")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Invalid market snapshot store",
            )
        })?;
    let current_tokens = current.as_object();
    let mut previous_snapshot = serde_json::Map::new();
    let mut current_snapshot = serde_json::Map::new();
    let mut comparison = serde_json::Map::new();

    for address in token_addresses {
        let previous_entry = stored_tokens.get(address).cloned();
        let previous_data = previous_entry.as_ref().and_then(|entry| entry.get("data"));
        let current_data = current_tokens.and_then(|tokens| tokens.get(address));
        if let Some(entry) = previous_entry.as_ref() {
            previous_snapshot.insert(address.clone(), entry.clone());
        }
        if let Some(data) = current_data {
            let entry = json!({"capturedAt": captured_at, "data": data});
            current_snapshot.insert(address.clone(), entry.clone());
            comparison.insert(
                address.clone(),
                json!({
                    "baselineAvailable": previous_data.is_some(),
                    "previousCapturedAt": previous_entry.as_ref().and_then(|entry| entry.get("capturedAt")),
                    "currentCapturedAt": captured_at,
                    "metrics": metric_comparison(previous_data, Some(data))
                }),
            );
            stored_tokens.insert(address.clone(), entry);
        } else {
            comparison.insert(
                address.clone(),
                json!({
                    "baselineAvailable": previous_data.is_some(),
                    "currentAvailable": false,
                    "metrics": {}
                }),
            );
        }
    }

    stored["updatedAt"] = Value::String(captured_at);
    let parent = path
        .parent()
        .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "Invalid snapshot path"))?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let temp = path.with_extension("json.tmp");
    tokio::fs::write(
        &temp,
        serde_json::to_vec_pretty(&stored)
            .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?,
    )
    .await
    .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    tokio::fs::rename(temp, path)
        .await
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(json!({
        "previous": previous_snapshot,
        "current": current_snapshot,
        "comparison": comparison
    }))
}

fn rpc_error(value: &Value) -> Option<ApiError> {
    value.get("error").map(|provider_error| {
        error(
            StatusCode::BAD_GATEWAY,
            format!("Solana RPC request failed: {provider_error}"),
        )
    })
}

fn normalize_token_accounts(value: &Value) -> Vec<Value> {
    value["result"]["value"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|account| {
            let info = &account["account"]["data"]["parsed"]["info"];
            let amount = &info["tokenAmount"];
            let mint = info["mint"].as_str()?;
            Some(json!({
                "mint": mint,
                "token_account": account["pubkey"],
                "amount": amount["amount"],
                "decimals": amount["decimals"],
                "ui_amount": amount["uiAmountString"]
            }))
        })
        .collect()
}

pub async fn get_wallet_holdings(Json(input): Json<WalletHoldingsRequest>) -> ApiResult {
    validate_address(&input.wallet_address, "wallet_address")?;
    let url = env_url("SOLANA_RPC_URL", DEFAULT_SOLANA_RPC_URL);
    let http = client()?;
    let native_request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "getBalance",
        "params": [input.wallet_address, {"commitment": "confirmed"}]
    });
    let token_request = json!({
        "jsonrpc": "2.0", "id": 2, "method": "getTokenAccountsByOwner",
        "params": [input.wallet_address, {"programId": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"}, {"encoding": "jsonParsed", "commitment": "confirmed"}]
    });
    let token_2022_request = json!({
        "jsonrpc": "2.0", "id": 3, "method": "getTokenAccountsByOwner",
        "params": [input.wallet_address, {"programId": "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"}, {"encoding": "jsonParsed", "commitment": "confirmed"}]
    });
    let (native, tokens, token_2022) = tokio::try_join!(
        async {
            json_response(
                http.post(&url)
                    .json(&native_request)
                    .send()
                    .await
                    .map_err(|e| error(StatusCode::BAD_GATEWAY, e))?,
            )
            .await
        },
        async {
            json_response(
                http.post(&url)
                    .json(&token_request)
                    .send()
                    .await
                    .map_err(|e| error(StatusCode::BAD_GATEWAY, e))?,
            )
            .await
        },
        async {
            json_response(
                http.post(&url)
                    .json(&token_2022_request)
                    .send()
                    .await
                    .map_err(|e| error(StatusCode::BAD_GATEWAY, e))?,
            )
            .await
        }
    )?;
    if let Some(err) = rpc_error(&native)
        .or_else(|| rpc_error(&tokens))
        .or_else(|| rpc_error(&token_2022))
    {
        return Err(err);
    }
    let lamports = native["result"]["value"].as_u64().unwrap_or_default();
    let mut holdings = normalize_token_accounts(&tokens);
    holdings.extend(normalize_token_accounts(&token_2022));
    Ok(Json(json!({
        "skill": "getWalletHoldings",
        "wallet_address": input.wallet_address,
        "native_balance": {"lamports": lamports, "sol": lamports as f64 / 1_000_000_000.0},
        "holdings": holdings,
        "provider": "solana-rpc"
    })))
}

pub async fn get_market_data(Json(input): Json<MarketDataRequest>) -> ApiResult {
    if input.token_addresses.is_empty() || input.token_addresses.len() > 50 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "token_addresses must contain 1 to 50 addresses",
        ));
    }
    for address in &input.token_addresses {
        validate_address(address, "token_addresses")?;
    }
    let url = format!(
        "{}/price/v3",
        env_url("JUPITER_API_URL", DEFAULT_JUPITER_API_URL)
    );
    let mut request = client()?
        .get(url)
        .query(&[("ids", input.token_addresses.join(","))]);
    if let Ok(key) = std::env::var("JUPITER_API_KEY") {
        request = request.header("x-api-key", key);
    }
    let data = json_response(
        request
            .send()
            .await
            .map_err(|e| error(StatusCode::BAD_GATEWAY, e))?,
    )
    .await?;
    let snapshots = snapshot_market_data(&input.token_addresses, &data).await?;
    Ok(Json(json!({
        "skill": "getMarketData",
        "provider": "jupiter",
        "data": data,
        "snapshots": snapshots,
        "comparisonSource": "skill-managed rolling snapshots"
    })))
}

pub async fn get_token_metadata(Json(input): Json<TokenRequest>) -> ApiResult {
    validate_address(&input.token_address, "token_address")?;
    let url = format!(
        "{}/tokens/v2/search",
        env_url("JUPITER_API_URL", DEFAULT_JUPITER_API_URL)
    );
    let mut request = client()?.get(url).query(&[("query", &input.token_address)]);
    if let Ok(key) = std::env::var("JUPITER_API_KEY") {
        request = request.header("x-api-key", key);
    }
    let data = json_response(
        request
            .send()
            .await
            .map_err(|e| error(StatusCode::BAD_GATEWAY, e))?,
    )
    .await?;
    Ok(Json(
        json!({"skill": "getTokenMetadata", "provider": "jupiter", "data": data}),
    ))
}

pub async fn get_liquidity(Json(input): Json<TokenRequest>) -> ApiResult {
    validate_address(&input.token_address, "token_address")?;
    let key = std::env::var("BIRDEYE_API_KEY").map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "BIRDEYE_API_KEY is required for getLiquidity",
        )
    })?;
    let url = format!(
        "{}/defi/v3/token/exit-liquidity",
        env_url("BIRDEYE_API_URL", DEFAULT_BIRDEYE_API_URL)
    );
    let response = client()?
        .get(url)
        .header("X-API-KEY", key)
        .header("x-chain", "solana")
        .query(&[("address", &input.token_address)])
        .send()
        .await
        .map_err(|e| error(StatusCode::BAD_GATEWAY, e))?;
    let data = json_response(response).await?;
    Ok(Json(
        json!({"skill": "getLiquidity", "provider": "birdeye", "data": data}),
    ))
}

pub async fn get_protocol_events(Json(input): Json<ProtocolEventsRequest>) -> ApiResult {
    validate_address(&input.address, "address")?;
    if input.limit == 0 || input.limit > 100 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "limit must be between 1 and 100",
        ));
    }
    let key = std::env::var("HELIUS_API_KEY").map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "HELIUS_API_KEY is required for getProtocolEvents",
        )
    })?;
    let url = format!(
        "{}/v0/addresses/{}/transactions",
        env_url("HELIUS_API_URL", DEFAULT_HELIUS_API_URL),
        input.address
    );
    let response = client()?
        .get(url)
        .query(&[("api-key", key), ("limit", input.limit.to_string())])
        .send()
        .await
        .map_err(|e| error(StatusCode::BAD_GATEWAY, e))?;
    let data = json_response(response).await?;
    Ok(Json(
        json!({"skill": "getProtocolEvents", "provider": "helius", "address": input.address, "events": data}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_solana_addresses() {
        assert!(validate_address("So11111111111111111111111111111111111111112", "address").is_ok());
        assert!(validate_address("not-a-solana-address", "address").is_err());
    }

    #[test]
    fn normalizes_parsed_token_accounts() {
        let response = json!({"result":{"value":[{"pubkey":"account", "account":{"data":{"parsed":{"info":{"mint":"So11111111111111111111111111111111111111112","tokenAmount":{"amount":"12","decimals":1,"uiAmountString":"1.2"}}}}}}]}});
        let holdings = normalize_token_accounts(&response);
        assert_eq!(holdings[0]["ui_amount"], "1.2");
    }

    #[test]
    fn compares_market_metrics_from_two_snapshots() {
        let previous = json!({
            "usdPrice": 100.0,
            "liquidity": 1_000.0,
            "priceChange24h": -2.0
        });
        let current = json!({
            "usdPrice": 110.0,
            "liquidity": 900.0,
            "priceChange24h": 3.0
        });
        let comparison = metric_comparison(Some(&previous), Some(&current));
        assert_eq!(comparison["usdPrice"]["absoluteChange"], 10.0);
        assert_eq!(comparison["usdPrice"]["percentChange"], 10.0);
        assert_eq!(comparison["liquidity"]["percentChange"], -10.0);
        assert_eq!(comparison["priceChange24h"]["absoluteChange"], 5.0);
        assert!(comparison.get("volume24h").is_none());
    }
}
