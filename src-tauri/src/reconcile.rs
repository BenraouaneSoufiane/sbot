use crate::models::*;
use chrono::Utc;
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::process::Command;

fn demo_source(name: &str) -> Value {
    match name {
        "orders" => json!([
            {"order_id":"10492","amount":428.2,"currency":"USD"},
            {"order_id":"10501","amount":211.0,"currency":"USD"}
        ]),
        "payouts" => json!([
            {"order_id":"10492","amount":426.2,"currency":"USD"},
            {"order_id":"10501","amount":211.0,"currency":"USD"},
            {"order_id":"10501","amount":211.0,"currency":"USD"}
        ]),
        "inventory" => json!([{"sku":"SKU-100","system_quantity":42,"physical_count":42}]),
        _ => json!([]),
    }
}

fn source_instructions(source: &Source) -> String {
    if let Some(name) = source.url.strip_prefix("demo://") {
        let data =
            serde_json::to_string(&demo_source(name)).unwrap_or_else(|_| "[]".to_string());
        return format!(
            "- Source \"{}\" is built-in demo data (do not fetch it over HTTP):\n  {}",
            source.name, data
        );
    }
    let auth = if source.auth == "bearer" && !source.token.is_empty() {
        format!(
            "send the request header `Authorization: Bearer {}`",
            source.token
        )
    } else if source.auth == "custom-header" {
        let name = source.header_name.as_deref().unwrap_or("").trim();
        let value = source.header_value.as_deref().unwrap_or("");
        if name.is_empty() || value.is_empty() {
            "no usable authentication (custom header name/value are empty)".to_string()
        } else {
            format!("send the request header `{name}: {value}`")
        }
    } else {
        "no authentication".to_string()
    };
    format!(
        "- Source \"{}\": fetch with the http_request tool using GET {} ({}). Treat non-JSON bodies (CSV, plain text) as bounded text.",
        source.name, source.url, auth
    )
}

fn analysis_prompt(check: &Check) -> String {
    let sources = check
        .sources
        .iter()
        .map(source_instructions)
        .collect::<Vec<_>>()
        .join("\n");
    let wallet = if check.wallet_address.is_empty() {
        "No Solana wallet was configured.".to_string()
    } else {
        format!(
            "Solana wallet: {}. Use the project Solana skills (portfolio-balance, get-wallet-holdings, get-market-data, get-token-metadata, get-liquidity, get-protocol-events) or query the Solana JSON-RPC directly to retrieve the on-chain facts this check requires.",
            check.wallet_address
        )
    };
    let notifications = {
        let enabled: Vec<&Notification> =
            check.notifications.iter().filter(|n| n.enabled).collect();
        if enabled.is_empty() {
            "No notifications are configured for this check.".to_string()
        } else {
            let list = enabled
                .iter()
                .map(|n| {
                    let label = if n.label.is_empty() { &n.r#type } else { &n.label };
                    let channel_id = if n.r#type == "whatsapp" {
                        "whatsapp.sbot"
                    } else {
                        n.r#type.as_str()
                    };
                    format!(
                        "- {}: send with `zeroclaw channel send` using --channel-id {} --recipient {}",
                        label, channel_id, n.recipient
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "After the analysis, send a result summary to each configured notification using the shell tool:\n  `zeroclaw channel send \"<message>\" --channel-id <type> --recipient <recipient>`\nThe message must summarize the outcome: matched and exception counts, each exception with its title, detail, amount and severity, or a short confirmation when no exceptions were found. Always send the notification, even when there are no exceptions. Configured notifications:\n{}",
                list
            )
        }
    };
    format!(
        r#"You are a reconciliation analyst for the sbot workspace. Perform this check end-to-end and return a single strict JSON object with no prose outside it.

Check statement:
{}

{}

Data sources (fetch each one yourself with the http_request tool):
{}

Notifications:
{}

Return strict JSON with exactly these keys:
- summary (string): a one or two sentence result.
- records (integer): total records compared.
- matched (integer): records that matched.
- exceptions (array): each item {{"title","detail","amount","severity"}}.
- notifications (array of strings): the channel ids you actually notified (empty when none were required or none were sent).
- notificationError (string or null): any error raised while sending notifications.

Do not invent data. Never echo source credentials, tokens, or headers into your output. If a source cannot be fetched, report it as an exception rather than failing silently."#,
        check.prompt, wallet, sources, notifications
    )
}

fn reconciliation_prompt(check: &Check) -> String {
    analysis_prompt(check)
}

fn source_test_prompt(source: &Source) -> String {
    format!(
        r#"You validate a single data source for a reconciliation check. Fetch the source with the http_request tool and report its shape.

{}

Return strict JSON with exactly these keys:
- ok (boolean): true when the source was reachable.
- records (integer): number of records (for a JSON array, its length; otherwise 1).
- preview (array or string): the first two rows for an array, otherwise a bounded text preview.
- error (string or null): a short error when the source could not be loaded.

Do not invent data. Never echo credentials, tokens, or headers."#,
        source_instructions(source)
    )
}

async fn run_agent(prompt: &str) -> Result<String, String> {
    let mut command =
        Command::new(std::env::var("ZEROCLAW_BIN").unwrap_or_else(|_| "zeroclaw".into()));
    let agent = std::env::var("ZEROCLAW_AGENT").unwrap_or_else(|_| "reconcile".into());
    command
        .args(["agent", "-a", &agent, "-m", prompt])
        .stdin(Stdio::null());
    command.kill_on_drop(true);
    let output = command.output().await.map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_json_object(text: &str) -> Result<Value, String> {
    let start = text.find('{').ok_or("the agent did not return JSON")?;
    let end = text.rfind('}').ok_or("the agent did not return JSON")?;
    serde_json::from_str(&text[start..=end]).map_err(|e| e.to_string())
}

pub async fn test_source(source: &Source) -> Result<SourceTest, String> {
    let text = run_agent(&source_test_prompt(source)).await?;
    let value = parse_json_object(&text)?;
    let ok = value.get("ok").and_then(Value::as_bool).unwrap_or(true);
    if !ok {
        let error = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("The source could not be loaded");
        return Err(error.to_string());
    }
    let records = value.get("records").and_then(Value::as_u64).unwrap_or(0) as usize;
    let preview = value.get("preview").cloned().unwrap_or(Value::Null);
    Ok(SourceTest {
        ok: true,
        records,
        preview,
    })
}

fn readable(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Array(items) => items
            .iter()
            .map(readable)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(map) => {
            for key in [
                "summary",
                "message",
                "text",
                "content",
                "description",
                "result",
            ] {
                if let Some(v) = map.get(key) {
                    let text = readable(v);
                    if !text.is_empty() {
                        return text;
                    }
                }
            }
            map.iter()
                .filter_map(|(k, v)| {
                    let text = readable(v);
                    (!text.is_empty()).then(|| format!("{}: {text}", k.replace('_', " ")))
                })
                .collect::<Vec<_>>()
                .join(" · ")
        }
    }
}

fn numeric(value: Option<&Value>, keys: &[&str]) -> Option<u64> {
    let value = value?;
    if let Some(n) = value.as_u64() {
        return Some(n);
    }
    if let Some(s) = value.as_str() {
        return s.replace(',', "").trim().parse().ok();
    }
    value
        .as_object()
        .and_then(|map| keys.iter().find_map(|key| numeric(map.get(*key), keys)))
}

fn normalize(value: Value, mode: &str) -> ReconcileResult {
    let exceptions = value
        .get("exceptions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| ReconcileException {
            title: readable(item.get("title").unwrap_or(&Value::Null)),
            detail: readable(item.get("detail").unwrap_or(&Value::Null)),
            amount: readable(item.get("amount").unwrap_or(&Value::Null)),
            severity: readable(item.get("severity").unwrap_or(&Value::Null)),
        })
        .collect::<Vec<_>>();
    let records = numeric(
        value.get("records"),
        &["total", "count", "records", "value"],
    )
    .unwrap_or(0);
    let matched = numeric(
        value.get("matched"),
        &["total", "count", "matched", "value"],
    )
    .unwrap_or_else(|| records.saturating_sub(exceptions.len() as u64));
    let summary = value
        .get("summary")
        .map(readable)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Reconciliation completed.".into());
    let notified = value
        .get("notifications")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let notification_error = value
        .get("notificationError")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    ReconcileResult {
        summary,
        records,
        matched,
        exceptions,
        mode: mode.into(),
        notified,
        notification_error,
    }
}

pub async fn run_reconciliation(
    check: &Check,
    logs: &mut Vec<RunLog>,
) -> Result<ReconcileResult, String> {
    add_log(logs, "reconciliation", "Preparing reconciliation", "running");
    add_log(
        logs,
        "analysis",
        "Delegating to the ZeroClaw agent",
        "running",
    );
    let text = run_agent(&reconciliation_prompt(check)).await?;
    let result = normalize(parse_json_object(&text)?, "zeroclaw");
    add_log(
        logs,
        "analysis",
        &format!(
            "Comparison complete · {} matched, {} exceptions",
            result.matched,
            result.exceptions.len()
        ),
        "complete",
    );
    Ok(result)
}

pub fn add_log(logs: &mut Vec<RunLog>, step: &str, message: &str, status: &str) {
    logs.push(RunLog {
        id: format!("log-{}-{}", Utc::now().timestamp_millis(), logs.len()),
        step: step.into(),
        message: message.into(),
        status: status.into(),
        timestamp: Utc::now().to_rfc3339(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inlines_demo_sources() {
        assert_eq!(demo_source("orders").as_array().unwrap().len(), 2);
        assert_eq!(demo_source("payouts").as_array().unwrap().len(), 3);
        assert_eq!(demo_source("unknown").as_array().unwrap().len(), 0);
    }

    #[test]
    fn describes_http_sources_with_their_auth() {
        let bearer = Source {
            name: "Orders".into(),
            url: "https://example.test/orders".into(),
            auth: "bearer".into(),
            token: "secret".into(),
            ..Default::default()
        };
        let instructions = source_instructions(&bearer);
        assert!(instructions.contains("GET https://example.test/orders"));
        assert!(instructions.contains("Authorization: Bearer secret"));
        assert!(!instructions.contains("demo://"));
    }

    #[test]
    fn normalizes_agent_result_and_notifications() {
        let result = normalize(
            json!({
                "summary": "Three exceptions.",
                "records": 10,
                "matched": 7,
                "exceptions": [
                    {"title":"Dup","detail":"A","amount":"$1.00","severity":"high"},
                    {"title":"Missing","detail":"B","amount":"$2.00","severity":"medium"},
                    {"title":"Diff","detail":"C","amount":"$3.00","severity":"low"}
                ],
                "notifications": ["telegram", "email"],
                "notificationError": null
            }),
            "zeroclaw",
        );
        assert_eq!(result.records, 10);
        assert_eq!(result.matched, 7);
        assert_eq!(result.exceptions.len(), 3);
        assert_eq!(result.notified, vec!["telegram", "email"]);
        assert!(result.notification_error.is_none());
    }

    #[test]
    fn falls_back_to_exception_count_when_matched_is_missing() {
        let result = normalize(
            json!({
                "summary": "One exception.",
                "records": 5,
                "exceptions": [{"title":"X","detail":"Y","amount":"","severity":"low"}]
            }),
            "zeroclaw",
        );
        assert_eq!(result.matched, 4);
    }
}
