use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashMap;

fn deserialize_count<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::Number(number) => number
            .as_u64()
            .unwrap_or_else(|| number.as_f64().unwrap_or_default().max(0.0).round() as u64),
        Value::Array(items) => items.len() as u64,
        Value::Object(items) => items.len() as u64,
        Value::String(text) => text.parse().unwrap_or_default(),
        _ => 0,
    })
}

fn deserialize_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::Null => String::new(),
        Value::String(text) => text,
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        other => other.to_string(),
    })
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub auth: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub header_name: Option<String>,
    #[serde(default)]
    pub header_value: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub recipient: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub sender_mode: Option<String>,
    #[serde(default)]
    pub custom_sender: Option<String>,
    #[serde(default)]
    pub bot_mode: Option<String>,
    #[serde(default)]
    pub bot_token: Option<String>,
    #[serde(default)]
    pub phone_number_id: Option<String>,
    #[serde(default)]
    pub discord_channel_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleConfig {
    pub frequency: String,
    pub time: String,
    pub weekday: String,
    pub timezone: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub schedule: String,
    #[serde(default)]
    pub schedule_config: Option<ScheduleConfig>,
    #[serde(default)]
    pub last_run: String,
    #[serde(default)]
    pub last_run_key: Option<String>,
    #[serde(default)]
    pub match_rate: Option<f64>,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub wallet_address: String,
    #[serde(default)]
    pub sources: Vec<Source>,
    #[serde(default)]
    pub notifications: Vec<Notification>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunLog {
    pub id: String,
    pub step: String,
    pub message: String,
    pub status: String,
    pub timestamp: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub check_id: String,
    pub check: String,
    pub status: String,
    pub started_at: String,
    pub duration: String,
    #[serde(default, deserialize_with = "deserialize_count")]
    pub records: u64,
    #[serde(default, deserialize_with = "deserialize_count")]
    pub matched: u64,
    #[serde(default, deserialize_with = "deserialize_count")]
    pub exceptions: u64,
    #[serde(default, deserialize_with = "deserialize_text")]
    pub amount: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub logs: Vec<RunLog>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionItem {
    pub id: String,
    pub check_id: String,
    pub title: String,
    pub detail: String,
    #[serde(default, deserialize_with = "deserialize_text")]
    pub amount: String,
    pub severity: String,
    pub age: String,
    pub owner: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub r#type: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub destination: String,
    #[serde(default)]
    pub sender_mode: Option<String>,
    #[serde(default)]
    pub custom_sender: Option<String>,
    #[serde(default)]
    pub bot_mode: Option<String>,
    #[serde(default)]
    pub phone_number_id: Option<String>,
    #[serde(default)]
    pub discord_channel_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct AppData {
    #[serde(default)]
    pub connections: HashMap<String, Connection>,
    #[serde(default)]
    pub checks: Vec<Check>,
    #[serde(default)]
    pub runs: Vec<Run>,
    #[serde(default)]
    pub exceptions: Vec<ExceptionItem>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReconcileException {
    pub title: String,
    pub detail: String,
    pub amount: String,
    pub severity: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReconcileResult {
    pub summary: String,
    pub records: u64,
    pub matched: u64,
    pub exceptions: Vec<ReconcileException>,
    pub mode: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResponse {
    pub run: Run,
    pub result: ReconcileResult,
    pub notified: Vec<String>,
    pub notification_error: Option<String>,
}

#[derive(Serialize)]
pub struct SourceTest {
    pub ok: bool,
    pub records: usize,
    pub preview: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_legacy_run_arrays_and_numeric_amounts() {
        let run: Run = serde_json::from_value(serde_json::json!({
            "id": "legacy",
            "checkId": "check",
            "check": "Legacy check",
            "status": "complete",
            "startedAt": "Just now",
            "duration": "1s",
            "records": [{"id": 1}, {"id": 2}],
            "matched": [{"id": 1}],
            "exceptions": 1,
            "amount": 42.5
        }))
        .expect("legacy run should deserialize");

        assert_eq!(run.records, 2);
        assert_eq!(run.matched, 1);
        assert_eq!(run.exceptions, 1);
        assert_eq!(run.amount, "42.5");
    }

    #[test]
    fn reads_null_exception_amount() {
        let exception: ExceptionItem = serde_json::from_value(serde_json::json!({
            "id": "legacy",
            "checkId": "check",
            "title": "Missing amount",
            "detail": "Legacy record",
            "amount": null,
            "severity": "low",
            "age": "Just now",
            "owner": "Unassigned"
        }))
        .expect("legacy exception should deserialize");

        assert!(exception.amount.is_empty());
    }
}
