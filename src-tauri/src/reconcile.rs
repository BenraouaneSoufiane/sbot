use crate::models::*;
use chrono::Utc;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION},
    Client,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    process::Stdio,
    time::Duration,
};
use tokio::process::Command;

fn client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())
}

pub async fn fetch_source(source: &Source) -> Result<Value, String> {
    if let Some(name) = source.url.strip_prefix("demo://") {
        return Ok(match name {
            "orders" => {
                json!([{"order_id":"10492","amount":428.2,"currency":"USD"},{"order_id":"10501","amount":211.0,"currency":"USD"}])
            }
            "payouts" => {
                json!([{"order_id":"10492","amount":426.2,"currency":"USD"},{"order_id":"10501","amount":211.0,"currency":"USD"},{"order_id":"10501","amount":211.0,"currency":"USD"}])
            }
            "inventory" => json!([{"sku":"SKU-100","system_quantity":42,"physical_count":42}]),
            _ => json!([]),
        });
    }
    if !(source.url.starts_with("https://") || source.url.starts_with("http://")) {
        return Err(format!("Unsupported source URL: {}", source.url));
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/csv;q=0.9, */*;q=0.5"),
    );
    if source.auth == "bearer" && !source.token.is_empty() {
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", source.token))
                .map_err(|e| e.to_string())?,
        );
    }
    if source.auth == "custom-header" {
        let name = source.header_name.as_deref().unwrap_or("").trim();
        let value = source.header_value.as_deref().unwrap_or("");
        if name.is_empty() || value.is_empty() {
            return Err("Custom header name and value are required".into());
        }
        headers.insert(
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| "Invalid custom header name")?,
            HeaderValue::from_str(value).map_err(|_| "Invalid custom header value")?,
        );
    }
    let response = client()?
        .get(&source.url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("{} returned HTTP {}", source.name, status.as_u16()));
    }
    let text = response.text().await.map_err(|e| e.to_string())?;
    Ok(serde_json::from_str(&text)
        .unwrap_or_else(|_| Value::String(text.chars().take(100_000).collect())))
}

pub async fn test_source(source: &Source) -> Result<SourceTest, String> {
    let value = fetch_source(source).await?;
    let records = value.as_array().map_or(1, Vec::len);
    let preview = value
        .as_array()
        .map(|rows| Value::Array(rows.iter().take(2).cloned().collect()))
        .unwrap_or_else(|| Value::String(value.to_string().chars().take(200).collect()));
    Ok(SourceTest {
        ok: true,
        records,
        preview,
    })
}

fn local_reconcile(check: &Check, data: &[Value]) -> ReconcileResult {
    let records = data
        .iter()
        .map(|v| v.as_array().map_or(0, Vec::len))
        .sum::<usize>() as u64;
    let mut exceptions = Vec::new();
    if check.id == "stripe-settlement" {
        let orders = data
            .first()
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let payouts = data
            .get(1)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut seen = HashSet::new();
        for payout in payouts {
            let id = payout["order_id"].as_str().unwrap_or_default();
            let amount = payout["amount"].as_f64().unwrap_or_default();
            if !seen.insert(id.to_owned()) {
                exceptions.push(ReconcileException {
                    title: "Duplicate charge detected".into(),
                    detail: format!("Order #{id}"),
                    amount: format!("${amount:.2}"),
                    severity: "medium".into(),
                });
            }
            match orders
                .iter()
                .find(|order| order["order_id"].as_str() == Some(id))
            {
                None => exceptions.push(ReconcileException {
                    title: "Charge has no matching order".into(),
                    detail: format!("Order #{id}"),
                    amount: format!("${amount:.2}"),
                    severity: "high".into(),
                }),
                Some(order)
                    if (order["amount"].as_f64().unwrap_or_default() - amount).abs() > 1.0 =>
                {
                    let order_amount = order["amount"].as_f64().unwrap_or_default();
                    exceptions.push(ReconcileException {
                        title: "Order total differs from charge".into(),
                        detail: format!(
                            "Order #{id} · difference ${:.2}",
                            (order_amount - amount).abs()
                        ),
                        amount: format!("${order_amount:.2}"),
                        severity: "medium".into(),
                    });
                }
                _ => {}
            }
        }
    }
    let count = exceptions.len() as u64;
    ReconcileResult {
        summary: if count > 0 {
            format!("Found {count} exceptions requiring review.")
        } else {
            "All records matched.".into()
        },
        records,
        matched: records.saturating_sub(count),
        exceptions,
        mode: "demo".into(),
    }
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
    ReconcileResult {
        summary,
        records,
        matched,
        exceptions,
        mode: mode.into(),
    }
}

pub async fn run_reconciliation(
    check: &Check,
    logs: &mut Vec<RunLog>,
) -> Result<ReconcileResult, String> {
    add_log(
        logs,
        "reconciliation",
        "Preparing reconciliation",
        "running",
    );
    let mut values = Vec::new();
    for source in &check.sources {
        add_log(
            logs,
            "source",
            &format!("Loading {}", source.name),
            "running",
        );
        match fetch_source(source).await {
            Ok(value) => {
                let count = value.as_array().map_or(1, Vec::len);
                add_log(
                    logs,
                    "source",
                    &format!(
                        "Loaded {} · {} record{}",
                        source.name,
                        count,
                        if count == 1 { "" } else { "s" }
                    ),
                    "complete",
                );
                values.push(value);
            }
            Err(error) => {
                add_log(
                    logs,
                    "source",
                    &format!("Could not load {}: {error}", source.name),
                    "failed",
                );
                return Err(error);
            }
        }
    }
    add_log(
        logs,
        "analysis",
        "Comparing records against the check statement",
        "running",
    );
    if std::env::var("ZEROCLAW_ENABLED").as_deref() != Ok("true") {
        let result = local_reconcile(check, &values);
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
        return Ok(result);
    }
    let sources: HashMap<_, _> = check
        .sources
        .iter()
        .zip(values)
        .map(|(s, v)| (s.name.clone(), v))
        .collect();
    let wallet_context = if check.wallet_address.is_empty() {
        "No wallet was configured.".to_owned()
    } else {
        format!(
            "Solana wallet: {}. Use the project Solana skills to retrieve the facts required by the check.",
            check.wallet_address
        )
    };
    let prompt=format!("You are a reconciliation analyst. Follow this check exactly:\n{}\n\n{}\n\nAdvanced sources:\n{}\n\nReturn strict JSON with summary, records, matched, and exceptions (each: title, detail, amount, severity).",check.prompt,wallet_context,serde_json::to_string(&sources).map_err(|e|e.to_string())?);
    let mut command =
        Command::new(std::env::var("ZEROCLAW_BIN").unwrap_or_else(|_| "zeroclaw".into()));
    command
        .args([
            "agent",
            "-a",
            &std::env::var("ZEROCLAW_AGENT").unwrap_or_else(|_| "reconcile".into()),
            "-m",
            &prompt,
        ])
        .stdin(Stdio::null());
    command.kill_on_drop(true);
    let output = command.output().await.map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let start = text.find('{').ok_or("ZeroClaw did not return JSON")?;
    let end = text.rfind('}').ok_or("ZeroClaw did not return JSON")?;
    let result = normalize(
        serde_json::from_str(&text[start..=end]).map_err(|e| e.to_string())?,
        "zeroclaw",
    );
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

fn esc(value: &str) -> String {
    html_escape::encode_safe(value).to_string()
}
fn notification_text(check: &Check, result: &ReconcileResult) -> String {
    format!(
        "{}: {}\n\n{}",
        check.name,
        result.summary,
        result
            .exceptions
            .iter()
            .map(|e| format!("• {} — {} {}", e.title, e.detail, e.amount))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_owned();
    }
    let mut truncated = value
        .chars()
        .take(max.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

fn telegram_message(check: &Check, result: &ReconcileResult) -> String {
    let mut message = format!(
        "🚨 <b>RECONCILIATION ALERT</b>\n━━━━━━━━━━━━━━━━━━\n\n📋 <b>{}</b>\n{}\n\n⚠️ <b>{} EXCEPTIONS NEED REVIEW</b>\n",
        esc(&check.name),
        esc(&result.summary),
        result.exceptions.len()
    );
    for (index, exception) in result.exceptions.iter().take(8).enumerate() {
        message.push_str(&format!(
            "\n🔸 <b>{}. {}</b>\n   {}\n   💰 <b>{}</b>\n",
            index + 1,
            esc(&truncate_chars(&exception.title, 120)),
            esc(&truncate_chars(&exception.detail, 240)),
            esc(&exception.amount)
        ));
    }
    if result.exceptions.len() > 8 {
        message.push_str(&format!(
            "\n➕ <i>{} more exceptions are waiting in Reconsile.</i>\n",
            result.exceptions.len() - 8
        ));
    }
    message.push_str("\n🟢 <i>Automated by Reconsile</i>");
    message
}

fn whatsapp_message(check: &Check, result: &ReconcileResult) -> String {
    let mut message = format!(
        "🚨 *RECONCILIATION ALERT*\n━━━━━━━━━━━━━━━━━━\n\n📋 *{}*\n{}\n\n⚠️ *{} EXCEPTIONS NEED REVIEW*\n",
        truncate_chars(&check.name, 120),
        truncate_chars(&result.summary, 500),
        result.exceptions.len()
    );
    for (index, exception) in result.exceptions.iter().take(8).enumerate() {
        message.push_str(&format!(
            "\n🔸 *{}. {}*\n   {}\n   💰 *{}*\n",
            index + 1,
            truncate_chars(&exception.title, 120),
            truncate_chars(&exception.detail, 240),
            exception.amount
        ));
    }
    if result.exceptions.len() > 8 {
        message.push_str(&format!(
            "\n➕ _{} more exceptions are waiting in Reconsile._\n",
            result.exceptions.len() - 8
        ));
    }
    message.push_str("\n🔗 Review now: https://reconsile.online\n\n🟢 _Automated by Reconsile_");
    message
}

fn discord_payload(check: &Check, result: &ReconcileResult) -> Value {
    let fields = result
        .exceptions
        .iter()
        .take(6)
        .enumerate()
        .map(|(index, exception)| {
            json!({
                "name": format!("🔸 {}. {}", index + 1, truncate_chars(&exception.title, 220)),
                "value": format!("{}\n💰 **{}**", truncate_chars(&exception.detail, 350), exception.amount),
                "inline": false
            })
        })
        .collect::<Vec<_>>();
    let omitted = result.exceptions.len().saturating_sub(fields.len());
    let description = if omitted == 0 {
        truncate_chars(&result.summary, 1_500)
    } else {
        format!(
            "{}\n\n➕ *{} more exceptions are waiting in Reconsile.*",
            truncate_chars(&result.summary, 1_350),
            omitted
        )
    };
    json!({
        "content": "🚨 **Reconciliation alert**",
        "embeds": [{
            "title": truncate_chars(&check.name, 250),
            "url": "https://reconsile.online",
            "description": description,
            "color": 1546837,
            "fields": fields,
            "author": {"name": "Reconsile"},
            "footer": {"text": format!("⚠️ {} exceptions need review • Automated by Reconsile", result.exceptions.len())}
        }],
        "components": [{
            "type": 1,
            "components": [{
                "type": 2,
                "style": 5,
                "label": "Open Reconsile",
                "emoji": {"name": "🔎"},
                "url": "https://reconsile.online"
            }]
        }]
    })
}

fn notification_html(check: &Check, result: &ReconcileResult) -> String {
    let exception_count = result.exceptions.len();
    let cards = result
        .exceptions
        .iter()
        .enumerate()
        .map(|(index, exception)| {
            format!(
                r#"<tr>
                  <td class="card-pad" style="padding:0 42px 12px 42px;">
                    <table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0" style="border:1px solid #e1e5e3;border-radius:12px;border-collapse:separate;">
                      <tr>
                        <td width="48" valign="top" style="padding:20px 0 20px 20px;">
                          <div style="width:38px;height:38px;line-height:38px;text-align:center;border-radius:10px;background:#fff0e4;color:#bd602d;font-size:16px;font-weight:700;">{}</div>
                        </td>
                        <td valign="top" style="padding:20px 12px 20px 20px;">
                          <div style="color:#18201d;font-size:16px;line-height:22px;font-weight:700;">{}</div>
                          <div style="padding-top:5px;color:#6d7772;font-size:14px;line-height:21px;">{}</div>
                        </td>
                        <td width="70" valign="top" align="right" style="padding:22px 20px 20px 4px;color:#18201d;font-size:14px;line-height:20px;font-weight:700;white-space:nowrap;">{}</td>
                      </tr>
                    </table>
                  </td>
                </tr>"#,
                index + 1,
                esc(&exception.title),
                esc(&exception.detail),
                esc(&exception.amount)
            )
        })
        .collect::<String>();

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <meta name="x-apple-disable-message-reformatting">
  <title>Reconciliation alert</title>
  <style>
    @media only screen and (max-width:620px) {{
      .email-shell {{ width:100% !important; }}
      .outer-pad {{ padding-left:12px !important; padding-right:12px !important; }}
      .brand-pad {{ padding-left:20px !important; }}
      .hero-pad {{ padding:32px 24px !important; }}
      .content-pad {{ padding:32px 24px 18px !important; }}
      .card-pad {{ padding-left:24px !important; padding-right:24px !important; }}
    }}
  </style>
</head>
<body style="margin:0;padding:0;background:#f2f6f4;color:#18201d;font-family:Arial,Helvetica,sans-serif;">
  <div style="display:none;max-height:0;overflow:hidden;opacity:0;">{} exceptions need review in {}.</div>
  <table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0" style="background:#f2f6f4;">
    <tr>
      <td class="outer-pad" align="center" style="padding:30px 20px 0;">
        <table class="email-shell" role="presentation" width="700" cellspacing="0" cellpadding="0" border="0" style="width:700px;max-width:700px;">
          <tr>
            <td class="brand-pad" style="padding:0 0 26px 10px;font-size:24px;line-height:30px;font-weight:700;color:#18201d;">
              <span style="color:#a8e47c;font-size:27px;vertical-align:1px;">◆</span><span style="color:#58ba8a;font-size:22px;margin-left:-8px;vertical-align:-6px;">◆</span>
              <span style="padding-left:10px;">Reconsile</span>
            </td>
          </tr>
          <tr>
            <td class="hero-pad" style="padding:36px 42px 34px;background:#12251f;border-radius:20px 20px 0 0;">
              <div style="color:#a9ed82;font-size:13px;line-height:18px;font-weight:700;letter-spacing:1.4px;text-transform:uppercase;">Reconciliation alert</div>
              <h1 style="margin:17px 0 12px;color:#ffffff;font-size:30px;line-height:38px;font-weight:700;">{}</h1>
              <div style="color:#b9c6c1;font-size:16px;line-height:25px;">{}</div>
            </td>
          </tr>
          <tr>
            <td style="background:#ffffff;border-radius:0 0 20px 20px;padding-bottom:38px;">
              <table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0">
                <tr>
                  <td class="content-pad" style="padding:34px 42px 18px;color:#187b56;font-size:13px;line-height:18px;font-weight:700;letter-spacing:1.2px;text-transform:uppercase;">{} exceptions need review</td>
                </tr>
                {}
                <tr>
                  <td align="center" style="padding:18px 24px 5px;">
                    <a href="https://reconsile.online" style="display:inline-block;padding:15px 24px;border-radius:10px;background:#197a55;color:#ffffff;text-decoration:none;font-size:15px;line-height:20px;font-weight:700;">Open Reconsile &#8594;</a>
                  </td>
                </tr>
              </table>
            </td>
          </tr>
          <tr>
            <td align="center" style="padding:25px 16px 30px;color:#8b9691;font-size:12px;line-height:19px;">
              Automated notification from Reconsile<br>
              Sent because this check has email notifications enabled.
            </td>
          </tr>
        </table>
      </td>
    </tr>
  </table>
</body>
</html>"#,
        exception_count,
        esc(&check.name),
        esc(&check.name),
        esc(&result.summary),
        exception_count,
        cards
    )
}

pub async fn send_notifications(
    check: &mut Check,
    result: &ReconcileResult,
    connections: &HashMap<String, Connection>,
) -> Result<Vec<String>, String> {
    if result.exceptions.is_empty() {
        return Ok(vec![]);
    }
    let mut sent = vec![];
    let mut failed = vec![];
    let check_context = check.clone();
    for notification in check.notifications.iter_mut().filter(|n| n.enabled) {
        let outcome = match notification.r#type.as_str() {
            "telegram" => {
                send_telegram(
                    notification,
                    connections.get("telegram"),
                    &check_context,
                    result,
                )
                .await
            }
            "discord" => {
                send_discord(
                    notification,
                    connections.get("discord"),
                    &check_context,
                    result,
                )
                .await
            }
            "whatsapp" => {
                send_whatsapp(
                    notification,
                    connections.get("whatsapp"),
                    &check_context,
                    result,
                )
                .await
            }
            "email"
                if notification.sender_mode.as_deref().unwrap_or("reconsile") == "reconsile" =>
            {
                send_email(notification, &check_context, result).await
            }
            other => {
                send_zeroclaw_channel(
                    other,
                    notification,
                    &notification_text(&check_context, result),
                )
                .await
            }
        };
        match outcome {
            Ok(channel) => sent.push(channel),
            Err(error) => failed.push(format!(
                "{}: {error}",
                if notification.label.is_empty() {
                    &notification.r#type
                } else {
                    &notification.label
                }
            )),
        }
    }
    if failed.is_empty() {
        Ok(sent)
    } else {
        Err(format!(
            "{}{}",
            if sent.is_empty() {
                String::new()
            } else {
                format!("{} sent. ", sent.join(", "))
            },
            failed.join("; ")
        ))
    }
}

async fn send_telegram(
    n: &Notification,
    saved: Option<&Connection>,
    check: &Check,
    result: &ReconcileResult,
) -> Result<String, String> {
    let hosted = n.bot_mode.as_deref().unwrap_or("reconsile") != "custom";
    let token = if hosted {
        std::env::var("TELEGRAM_BOT_TOKEN").unwrap_or_default()
    } else {
        n.bot_token
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| saved.map(|c| c.token.clone()))
            .unwrap_or_default()
    };
    if token.is_empty() {
        return Err(if hosted {
            "Reconsile Telegram notifications require TELEGRAM_BOT_TOKEN to be configured."
        } else {
            "Telegram is not connected. Add the bot token under Connections."
        }
        .into());
    }
    let recipient = if n.recipient.is_empty() {
        saved.map(|c| c.destination.as_str()).unwrap_or("")
    } else {
        &n.recipient
    };
    let chat = if recipient.parse::<i64>().is_ok() {
        recipient.to_owned()
    } else {
        let username = recipient.trim_start_matches('@').to_lowercase();
        let data: Value = client()?
            .get(format!("https://api.telegram.org/bot{token}/getUpdates"))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        data["result"]
            .as_array()
            .and_then(|rows| {
                rows.iter().rev().find_map(|row| {
                    let chat = &row["message"]["chat"];
                    (chat["username"].as_str()?.to_lowercase() == username)
                        .then(|| chat["id"].to_string())
                })
            })
            .ok_or_else(|| {
                format!("Telegram cannot find @{username}. Send the bot /start, then try again.")
            })?
    };
    let message = telegram_message(check, result);
    let response=client()?.post(format!("https://api.telegram.org/bot{token}/sendMessage")).json(&json!({"chat_id":chat,"text":message,"parse_mode":"HTML","link_preview_options":{"is_disabled":true},"reply_markup":{"inline_keyboard":[[{"text":"🔎 Open Reconsile","url":"https://reconsile.online"}]]}})).send().await.map_err(|e|e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Telegram returned HTTP {}",
            response.status().as_u16()
        ));
    }
    Ok("telegram".into())
}

async fn send_discord(
    n: &mut Notification,
    saved: Option<&Connection>,
    check: &Check,
    result: &ReconcileResult,
) -> Result<String, String> {
    let hosted = n.bot_mode.as_deref().unwrap_or("reconsile") != "custom";
    let token = if hosted {
        std::env::var("DISCORD_BOT_TOKEN").unwrap_or_default()
    } else {
        n.bot_token
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| saved.map(|c| c.token.clone()))
            .unwrap_or_default()
    };
    if token.is_empty() {
        return Err("Discord is not connected. Add a bot token under Connections.".into());
    }
    let recipient = if n.recipient.is_empty() {
        saved.map(|c| c.destination.as_str()).unwrap_or("")
    } else {
        &n.recipient
    };
    if !recipient.chars().all(|c| c.is_ascii_digit()) || recipient.is_empty() {
        return Err("Discord requires a numeric user ID.".into());
    }
    let channel = n
        .discord_channel_id
        .clone()
        .or_else(|| saved.and_then(|c| c.discord_channel_id.clone()));
    let auth = format!("Bot {token}");
    let channel = match channel {
        Some(id) => id,
        None => {
            let response = client()?
                .post("https://discord.com/api/v10/users/@me/channels")
                .header("authorization", &auth)
                .json(&json!({"recipient_id":recipient}))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err(format!(
                    "Discord could not create the DM (HTTP {})",
                    response.status().as_u16()
                ));
            }
            response.json::<Value>().await.map_err(|e| e.to_string())?["id"]
                .as_str()
                .ok_or("Discord did not return a DM channel ID")?
                .to_owned()
        }
    };
    n.discord_channel_id = Some(channel.clone());
    let response = client()?
        .post(format!(
            "https://discord.com/api/v10/channels/{channel}/messages"
        ))
        .header("authorization", auth)
        .json(&discord_payload(check, result))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Discord returned HTTP {}",
            response.status().as_u16()
        ));
    }
    Ok("discord".into())
}

async fn send_whatsapp(
    n: &Notification,
    saved: Option<&Connection>,
    check: &Check,
    result: &ReconcileResult,
) -> Result<String, String> {
    let hosted = n.bot_mode.as_deref().unwrap_or("reconsile") != "custom";
    let token = if hosted {
        std::env::var("WHATSAPP_BOT_TOKEN").unwrap_or_default()
    } else {
        n.bot_token
            .clone()
            .filter(|token| !token.is_empty())
            .or_else(|| saved.map(|connection| connection.token.clone()))
            .unwrap_or_default()
    };
    let phone_number_id = if hosted {
        std::env::var("WHATSAPP_PHONE_NUMBER_ID").unwrap_or_default()
    } else {
        n.phone_number_id
            .clone()
            .filter(|value| !value.is_empty())
            .or_else(|| saved.and_then(|connection| connection.phone_number_id.clone()))
            .unwrap_or_default()
    };
    if token.is_empty() {
        return Err(if hosted {
            "Reconsile WhatsApp notifications require WHATSAPP_BOT_TOKEN to be configured."
        } else {
            "WhatsApp is not connected. Add the bot token under Connections."
        }
        .into());
    }
    if phone_number_id.is_empty() {
        return Err(if hosted {
            "Reconsile WhatsApp notifications require WHATSAPP_PHONE_NUMBER_ID to be configured."
        } else {
            "WhatsApp requires the custom bot's Phone Number ID."
        }
        .into());
    }
    let recipient = if n.recipient.is_empty() {
        saved
            .map(|connection| connection.destination.as_str())
            .unwrap_or("")
    } else {
        &n.recipient
    };
    if recipient.trim().is_empty() {
        return Err("WhatsApp requires a phone number or group ID.".into());
    }
    let message = whatsapp_message(check, result);
    let template_name = std::env::var("WHATSAPP_TEMPLATE_NAME").unwrap_or_default();
    let payload = if template_name.is_empty() {
        json!({
            "messaging_product": "whatsapp",
            "recipient_type": "individual",
            "to": recipient.trim_start_matches('+'),
            "type": "text",
            "text": {"preview_url": false, "body": &message}
        })
    } else {
        json!({
            "messaging_product": "whatsapp",
            "to": recipient.trim_start_matches('+'),
            "type": "template",
            "template": {
                "name": template_name,
                "language": {"code": std::env::var("WHATSAPP_TEMPLATE_LANGUAGE").unwrap_or_else(|_| "en_US".into())},
                "components": [{"type": "body", "parameters": [{"type": "text", "text": &message}]}]
            }
        })
    };
    let graph_version =
        std::env::var("WHATSAPP_GRAPH_API_VERSION").unwrap_or_else(|_| "v25.0".into());
    let response = client()?
        .post(format!(
            "https://graph.facebook.com/{graph_version}/{phone_number_id}/messages"
        ))
        .bearer_auth(token)
        .json(&payload)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let body: Value = response.json().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        let detail = body["error"]["message"]
            .as_str()
            .unwrap_or("Meta rejected the message");
        return Err(format!(
            "WhatsApp returned HTTP {}: {detail}",
            status.as_u16()
        ));
    }
    Ok("whatsapp".into())
}

async fn send_email(
    n: &Notification,
    check: &Check,
    result: &ReconcileResult,
) -> Result<String, String> {
    let key = std::env::var("BREVO_API_KEY").unwrap_or_default();
    if key.is_empty() {
        return Err("Hosted email notifications require BREVO_API_KEY to be configured.".into());
    }
    let html = notification_html(check, result);
    let text = notification_text(check, result);
    let response=client()?.post("https://api.brevo.com/v3/smtp/email").header("api-key",key).json(&json!({"sender":{"name":"Reconsile","email":"notifications@reconsile.online"},"to":[{"email":n.recipient}],"subject":format!("{}: {} exceptions found",check.name,result.exceptions.len()),"htmlContent":html,"textContent":text})).send().await.map_err(|e|e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Brevo returned HTTP {}",
            response.status().as_u16()
        ));
    }
    Ok("email".into())
}

async fn send_zeroclaw_channel(
    channel: &str,
    n: &Notification,
    message: &str,
) -> Result<String, String> {
    if std::env::var("ZEROCLAW_ENABLED").as_deref() != Ok("true") {
        return Err(format!(
            "{} notifications require ZeroClaw to be enabled.",
            if n.label.is_empty() {
                channel
            } else {
                &n.label
            }
        ));
    }
    let mut command =
        Command::new(std::env::var("ZEROCLAW_BIN").unwrap_or_else(|_| "zeroclaw".into()));
    command
        .args([
            "channel",
            "send",
            message,
            "--channel-id",
            channel,
            "--recipient",
            &n.recipient,
        ])
        .stdin(Stdio::null());
    command.kill_on_drop(true);
    let status = command.status().await.map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!(
            "Notification exited {}",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(channel.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_template_is_branded_and_escapes_dynamic_content() {
        let check = Check {
            name: "Stripe <settlement>".into(),
            ..Default::default()
        };
        let result = ReconcileResult {
            summary: "2 records & one warning".into(),
            records: 2,
            matched: 1,
            exceptions: vec![ReconcileException {
                title: "Amount <difference>".into(),
                detail: "Expected $5 & received $4".into(),
                amount: "$1.00".into(),
                severity: "high".into(),
            }],
            mode: "test".into(),
        };

        let html = notification_html(&check, &result);
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("Reconciliation alert"));
        assert!(html.contains("Open Reconsile"));
        assert!(html.contains("Stripe &lt;settlement&gt;"));
        assert!(html.contains("2 records &amp; one warning"));
        assert!(html.contains("Amount &lt;difference&gt;"));
        assert!(!html.contains("Amount <difference>"));
    }

    fn notification_fixture() -> (Check, ReconcileResult) {
        (
            Check {
                name: "Stripe <settlement>".into(),
                ..Default::default()
            },
            ReconcileResult {
                summary: "Orders & payouts checked".into(),
                records: 2,
                matched: 1,
                exceptions: vec![ReconcileException {
                    title: "Amount difference".into(),
                    detail: "Payout differs by $2".into(),
                    amount: "$2.00".into(),
                    severity: "high".into(),
                }],
                mode: "test".into(),
            },
        )
    }

    #[test]
    fn telegram_notification_is_structured_and_html_safe() {
        let (check, result) = notification_fixture();
        let message = telegram_message(&check, &result);
        assert!(message.contains("🚨 <b>RECONCILIATION ALERT</b>"));
        assert!(message.contains("Stripe &lt;settlement&gt;"));
        assert!(message.contains("Orders &amp; payouts checked"));
        assert!(message.contains("🔸 <b>1. Amount difference</b>"));
    }

    #[test]
    fn whatsapp_notification_has_native_formatting_and_link() {
        let (check, result) = notification_fixture();
        let message = whatsapp_message(&check, &result);
        assert!(message.contains("🚨 *RECONCILIATION ALERT*"));
        assert!(message.contains("💰 *$2.00*"));
        assert!(message.contains("https://reconsile.online"));
    }

    #[test]
    fn discord_notification_uses_embed_and_link_button() {
        let (check, result) = notification_fixture();
        let payload = discord_payload(&check, &result);
        assert_eq!(payload["embeds"][0]["color"], 1546837);
        assert_eq!(payload["embeds"][0]["fields"][0]["inline"], false);
        assert_eq!(payload["components"][0]["components"][0]["style"], 5);
        assert_eq!(
            payload["components"][0]["components"][0]["url"],
            "https://reconsile.online"
        );
    }

    #[test]
    fn demo_source_and_reconcile() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let check = Check {
                    id: "stripe-settlement".into(),
                    sources: vec![
                        Source {
                            name: "orders".into(),
                            url: "demo://orders".into(),
                            ..Default::default()
                        },
                        Source {
                            name: "payouts".into(),
                            url: "demo://payouts".into(),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                };
                let mut logs = vec![];
                let result = run_reconciliation(&check, &mut logs).await.unwrap();
                assert_eq!(result.records, 5);
                assert_eq!(result.exceptions.len(), 2);
                assert!(logs.iter().any(|log| log.step == "analysis"));
            });
    }
}
