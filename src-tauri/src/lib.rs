mod models;
mod reconcile;
mod solana_skills;
mod store;
mod whatsapp;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{Datelike, Timelike, Utc};
use chrono_tz::Tz;
use models::*;
use reconcile::{add_log, run_reconciliation, send_notifications, test_source};
use serde_json::{json, Value};
use sha2::Digest;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex as StdMutex,
    },
    time::{Duration, Instant},
};
use store::Store;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

#[derive(Clone)]
struct AppService {
    store: Store,
    active_runs: Arc<StdMutex<HashSet<String>>>,
    run_cancellations: Arc<StdMutex<HashMap<String, Arc<AtomicBool>>>>,
    inbound_context: Arc<StdMutex<HashMap<String, Vec<String>>>>,
}

struct ActiveRunGuard {
    check_id: String,
    active_runs: Arc<StdMutex<HashSet<String>>>,
    run_cancellations: Arc<StdMutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl Drop for ActiveRunGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active_runs.lock() {
            active.remove(&self.check_id);
        }
        if let Ok(mut cancellations) = self.run_cancellations.lock() {
            cancellations.remove(&self.check_id);
        }
    }
}

type WebState = Arc<AppService>;
type WebResult<T> = Result<Json<T>, (StatusCode, Json<Value>)>;

fn web_error(status: StatusCode, error: impl ToString) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "error": error.to_string() })))
}

impl AppService {
    async fn state(&self) -> AppData {
        self.store.data.lock().await.clone()
    }

    async fn save_check(&self, mut check: Check) -> Result<Check, String> {
        let snapshot = {
            let mut data = self.store.data.lock().await;
            if check.id.is_empty() {
                check.id = format!("check-{}", Utc::now().timestamp_millis());
                check.status = "draft".into();
                check.last_run = "Never".into();
                check.match_rate = None;
            }
            if let Some(index) = data.checks.iter().position(|item| item.id == check.id) {
                data.checks[index] = check.clone();
            } else {
                data.checks.insert(0, check.clone());
            }
            data.clone()
        };
        self.store.save_data(&snapshot).await?;
        Ok(check)
    }

    async fn save_connection(&self, connection: Connection) -> Result<Connection, String> {
        if !["gmail", "telegram", "discord", "whatsapp"].contains(&connection.r#type.as_str()) {
            return Err("Unsupported connection type".into());
        }
        let snapshot = {
            let mut data = self.store.data.lock().await;
            data.connections
                .insert(connection.r#type.clone(), connection.clone());
            data.clone()
        };
        self.store.save_data(&snapshot).await?;
        Ok(connection)
    }

    async fn log_inbound_message(
        &self,
        channel: &str,
        message_id: &str,
        sender: &str,
        text: &str,
    ) -> Result<(), String> {
        let digest = hex::encode(sha2::Sha256::digest(message_id.as_bytes()));
        let run_id = format!("inbound-{channel}-{digest}");
        let snapshot = {
            let mut data = self.store.data.lock().await;
            if data.runs.iter().any(|run| run.id == run_id) {
                return Ok(());
            }
            let mut run = Run {
                id: run_id,
                check_id: format!("{channel}-inbound"),
                check: format!("{} inbound message", capitalize(channel)),
                status: "complete".into(),
                started_at: "Just now".into(),
                duration: "—".into(),
                records: 1,
                amount: "$0.00".into(),
                mode: Some(format!("{channel}-inbound")),
                ..Default::default()
            };
            add_log(
                &mut run.logs,
                "message",
                &format!("From {sender}: {text}"),
                "complete",
            );
            data.runs.insert(0, run);
            data.clone()
        };
        self.store.save_data(&snapshot).await
    }

    async fn append_inbound_log(
        &self,
        channel: &str,
        message_id: &str,
        step: &str,
        message: &str,
        status: &str,
    ) -> Result<(), String> {
        let digest = hex::encode(sha2::Sha256::digest(message_id.as_bytes()));
        let run_id = format!("inbound-{channel}-{digest}");
        let snapshot = {
            let mut data = self.store.data.lock().await;
            let run = data
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or("Inbound history entry not found")?;
            add_log(&mut run.logs, step, message, status);
            data.clone()
        };
        self.store.save_data(&snapshot).await
    }

    async fn delete_check(&self, id: &str) -> Result<(), String> {
        let snapshot = {
            let mut data = self.store.data.lock().await;
            if !data.checks.iter().any(|item| item.id == id) {
                return Err("Check not found".into());
            }
            data.checks.retain(|item| item.id != id);
            data.runs.retain(|item| item.check_id != id);
            data.exceptions.retain(|item| item.check_id != id);
            data.clone()
        };
        self.store.save_data(&snapshot).await
    }

    async fn delete_run(&self, id: &str) -> Result<(), String> {
        let snapshot = {
            let mut data = self.store.data.lock().await;
            if !data.runs.iter().any(|item| item.id == id) {
                return Err("Run not found".into());
            }
            data.runs.retain(|item| item.id != id);
            data.clone()
        };
        self.store.save_data(&snapshot).await
    }

    async fn delete_exception(&self, id: &str) -> Result<(), String> {
        let snapshot = {
            let mut data = self.store.data.lock().await;
            if !data.exceptions.iter().any(|item| item.id == id) {
                return Err("Exception not found".into());
            }
            data.exceptions.retain(|item| item.id != id);
            data.clone()
        };
        self.store.save_data(&snapshot).await
    }

    async fn execute_run(&self, id: &str, started_at: &str) -> Result<RunResponse, String> {
        {
            let mut active = self
                .active_runs
                .lock()
                .map_err(|_| "Run registry is unavailable")?;
            if !active.insert(id.to_owned()) {
                return Err("This check is already running. Wait for it to finish before starting another run.".into());
            }
        }
        let _active_run = ActiveRunGuard {
            check_id: id.to_owned(),
            active_runs: self.active_runs.clone(),
            run_cancellations: self.run_cancellations.clone(),
        };
        let cancellation = Arc::new(AtomicBool::new(false));
        self.run_cancellations
            .lock()
            .map_err(|_| "Run cancellation registry is unavailable")?
            .insert(id.to_owned(), cancellation.clone());
        let started = Instant::now();
        let timestamp = Utc::now().timestamp_millis();
        let (check, mut run, snapshot) = {
            let mut data = self.store.data.lock().await;
            let check = data
                .checks
                .iter()
                .find(|item| item.id == id)
                .cloned()
                .ok_or("Check not found")?;
            let mut run = Run {
                id: format!("run-{timestamp}"),
                check_id: check.id.clone(),
                check: check.name.clone(),
                status: "running".into(),
                started_at: started_at.into(),
                duration: "—".into(),
                amount: "$0.00".into(),
                ..Default::default()
            };
            add_log(
                &mut run.logs,
                "run",
                if started_at.starts_with("Scheduled") {
                    "Scheduled run started"
                } else {
                    "Manual run started"
                },
                "running",
            );
            data.runs.insert(0, run.clone());
            if let Some(saved) = data.checks.iter_mut().find(|item| item.id == id) {
                saved.status = "running".into();
            }
            (check, run, data.clone())
        };
        self.store.save_data(&snapshot).await?;
        let timeout_seconds = std::env::var("RUN_TIMEOUT_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(180);
        let result_attempt = {
            let reconciliation = tokio::time::timeout(
                Duration::from_secs(timeout_seconds),
                run_reconciliation(&check, &mut run.logs),
            );
            tokio::pin!(reconciliation);
            loop {
                tokio::select! {
                    result = &mut reconciliation => {
                        break result
                            .map_err(|_| format!("Run timed out after {timeout_seconds} seconds"))
                            .and_then(|result| result);
                    }
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        if cancellation.load(Ordering::Acquire) {
                            break Err("Run stopped by user".into());
                        }
                    }
                }
            }
        };
        let result = match result_attempt {
            Ok(value) => value,
            Err(error) => {
                run.status = if error == "Run stopped by user" {
                    "stopped"
                } else {
                    "failed"
                }
                .into();
                run.duration = format!("{}s", started.elapsed().as_secs().max(1));
                add_log(&mut run.logs, "run", &error, "failed");
                let snapshot = {
                    let mut data = self.store.data.lock().await;
                    if let Some(saved) = data.runs.iter_mut().find(|item| item.id == run.id) {
                        *saved = run.clone()
                    }
                    if let Some(saved) = data.checks.iter_mut().find(|item| item.id == id) {
                        saved.status = run.status.clone();
                        saved.last_run = "Just now".into();
                    }
                    data.clone()
                };
                self.store.save_data(&snapshot).await?;
                return Err(error);
            }
        };
        run.status = (if result.exceptions.is_empty() {
            "complete"
        } else {
            "attention"
        })
        .into();
        run.duration = format!("{}s", started.elapsed().as_secs().max(1));
        run.records = result.records;
        run.matched = result.matched;
        run.exceptions = result.exceptions.len() as u64;
        run.amount = result
            .exceptions
            .first()
            .map(|item| item.amount.clone())
            .unwrap_or_else(|| "$0.00".into());
        run.mode = Some(result.mode.clone());
        add_log(
            &mut run.logs,
            "result",
            &format!(
                "Saved {} records and {} exceptions",
                result.records,
                result.exceptions.len()
            ),
            "complete",
        );
        add_log(&mut run.logs, "run", "Run finished", "complete");
        let (mut notify_check, connections) = {
            let mut data = self.store.data.lock().await;
            if let Some(saved) = data.checks.iter_mut().find(|item| item.id == id) {
                saved.status = run.status.clone();
                saved.last_run = "Just now".into();
                saved.match_rate = Some(if result.records == 0 {
                    100.0
                } else {
                    (result.matched as f64 / result.records as f64 * 1000.0).round() / 10.0
                })
            }
            for (index, item) in result.exceptions.iter().enumerate() {
                data.exceptions.insert(
                    0,
                    ExceptionItem {
                        id: format!("EX-{}", timestamp + index as i64),
                        check_id: id.into(),
                        title: item.title.clone(),
                        detail: item.detail.clone(),
                        amount: item.amount.clone(),
                        severity: item.severity.clone(),
                        age: "Just now".into(),
                        owner: "Unassigned".into(),
                    },
                )
            }
            if let Some(saved) = data.runs.iter_mut().find(|item| item.id == run.id) {
                *saved = run.clone()
            }
            (
                data.checks
                    .iter()
                    .find(|item| item.id == id)
                    .cloned()
                    .unwrap(),
                data.connections.clone(),
            )
        };
        add_log(
            &mut run.logs,
            "notification",
            "Sending configured notifications",
            "running",
        );
        let notification_timeout = std::env::var("NOTIFICATION_TIMEOUT_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(30);
        let notification_result = tokio::time::timeout(
            Duration::from_secs(notification_timeout),
            send_notifications(&mut notify_check, &result, &connections),
        )
        .await
        .map_err(|_| format!("Notifications timed out after {notification_timeout} seconds"))
        .and_then(|result| result);
        let (notified, notification_error) = match notification_result {
            Ok(channels) => {
                add_log(
                    &mut run.logs,
                    "notification",
                    if channels.is_empty() {
                        "No notifications required"
                    } else {
                        "Notifications sent"
                    },
                    "complete",
                );
                (channels, None)
            }
            Err(error) => {
                add_log(&mut run.logs, "notification", &error, "failed");
                (vec![], Some(error))
            }
        };
        let snapshot = {
            let mut data = self.store.data.lock().await;
            if let Some(saved) = data.runs.iter_mut().find(|item| item.id == run.id) {
                *saved = run.clone()
            }
            if let Some(saved) = data.checks.iter_mut().find(|item| item.id == id) {
                saved.notifications = notify_check.notifications
            }
            data.clone()
        };
        self.store.save_data(&snapshot).await?;
        Ok(RunResponse {
            run,
            result,
            notified,
            notification_error,
        })
    }

    fn stop_run(&self, id: &str) -> Result<(), String> {
        let cancellations = self
            .run_cancellations
            .lock()
            .map_err(|_| "Run cancellation registry is unavailable")?;
        let cancellation = cancellations.get(id).ok_or("This check is not running")?;
        cancellation.store(true, Ordering::Release);
        Ok(())
    }
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

async fn get_state(State(service): State<WebState>) -> Json<AppData> {
    Json(service.state().await)
}
async fn health() -> Json<Value> {
    Json(
        json!({"ok":true,"zeroclaw":if std::env::var("ZEROCLAW_ENABLED").as_deref()==Ok("true"){"connected"}else{"demo"}}),
    )
}
async fn save_check(State(service): State<WebState>, Json(check): Json<Check>) -> WebResult<Check> {
    service
        .save_check(check)
        .await
        .map(Json)
        .map_err(|e| web_error(StatusCode::BAD_REQUEST, e))
}
async fn save_connection(
    State(service): State<WebState>,
    Json(connection): Json<Connection>,
) -> WebResult<Connection> {
    service
        .save_connection(connection)
        .await
        .map(Json)
        .map_err(|e| web_error(StatusCode::BAD_REQUEST, e))
}
async fn remove_check(State(service): State<WebState>, Path(id): Path<String>) -> WebResult<Value> {
    service
        .delete_check(&id)
        .await
        .map(|_| Json(json!({"ok":true})))
        .map_err(|e| web_error(StatusCode::NOT_FOUND, e))
}
async fn remove_run(State(service): State<WebState>, Path(id): Path<String>) -> WebResult<Value> {
    service
        .delete_run(&id)
        .await
        .map(|_| Json(json!({"ok":true})))
        .map_err(|e| web_error(StatusCode::NOT_FOUND, e))
}
async fn remove_exception(
    State(service): State<WebState>,
    Path(id): Path<String>,
) -> WebResult<Value> {
    service
        .delete_exception(&id)
        .await
        .map(|_| Json(json!({"ok":true})))
        .map_err(|e| web_error(StatusCode::NOT_FOUND, e))
}
async fn source_test(Json(source): Json<Source>) -> WebResult<SourceTest> {
    test_source(&source)
        .await
        .map(Json)
        .map_err(|e| web_error(StatusCode::BAD_REQUEST, e))
}
async fn run_check(
    State(service): State<WebState>,
    Path(id): Path<String>,
) -> WebResult<RunResponse> {
    // Keep the run alive if nginx or the browser closes the request before a
    // long reconciliation finishes. The active-run guard still prevents a
    // second run of the same check from starting concurrently.
    tokio::spawn(async move { service.execute_run(&id, "Just now").await })
        .await
        .map_err(|error| web_error(StatusCode::INTERNAL_SERVER_ERROR, error))?
        .map(Json)
        .map_err(|e| web_error(StatusCode::BAD_REQUEST, e))
}

fn schedule_slot(check: &Check) -> Option<String> {
    let schedule = check.schedule_config.as_ref()?;
    if !schedule.enabled || schedule.frequency == "manual" {
        return None;
    }
    let zone: Tz = schedule.timezone.parse().unwrap_or(chrono_tz::UTC);
    let now = Utc::now().with_timezone(&zone);
    let date = now.format("%Y-%m-%d").to_string();
    let time = now.format("%H:%M").to_string();
    match schedule.frequency.as_str() {
        "hourly" if now.minute() == 0 => Some(format!("{date}-{:02}", now.hour())),
        "daily" if time == schedule.time => Some(date),
        "weekly"
            if time == schedule.time
                && now.weekday().num_days_from_sunday().to_string() == schedule.weekday =>
        {
            Some(date)
        }
        _ => None,
    }
}
async fn scheduler(service: WebState) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    loop {
        interval.tick().await;
        let due = {
            let mut data = service.store.data.lock().await;
            let mut due = vec![];
            for check in &mut data.checks {
                if let Some(slot) = schedule_slot(check) {
                    if check.last_run_key.as_deref() != Some(&slot) {
                        check.last_run_key = Some(slot);
                        due.push(check.id.clone())
                    }
                }
            }
            let snapshot = data.clone();
            drop(data);
            let _ = service.store.save_data(&snapshot).await;
            due
        };
        for id in due {
            if let Err(error) = service.execute_run(&id, "Scheduled · just now").await {
                eprintln!("Scheduled check {id} failed: {error}")
            }
        }
    }
}

pub fn run() -> Result<(), String> {
    // Keep explicitly exported process variables authoritative, while filling in
    // local development and PM2 configuration from the repository's .env file.
    let _ = dotenvy::dotenv();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let store = Store::open().await?;
        store.recover_interrupted_runs().await?;
        let service = Arc::new(AppService {
            store,
            active_runs: Arc::new(StdMutex::new(HashSet::new())),
            run_cancellations: Arc::new(StdMutex::new(HashMap::new())),
            inbound_context: Arc::new(StdMutex::new(HashMap::new())),
        });
        tokio::spawn(scheduler(service.clone()));
        let web_root = std::env::var_os("WEB_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("web"));
        let index = web_root.join("index.html");
        let app = Router::new()
            .route("/api/state", get(get_state))
            .route("/api/health", get(health))
            .route("/api/checks", post(save_check))
            .route("/api/connections", post(save_connection))
            .route("/api/checks/{id}", delete(remove_check))
            .route("/api/runs/{id}", delete(remove_run))
            .route("/api/exceptions/{id}", delete(remove_exception))
            .route("/api/sources/test", post(source_test))
            .route("/api/checks/{id}/run", post(run_check))
            .route(
                "/api/webhooks/whatsapp",
                get(whatsapp::verify_webhook).post(whatsapp::receive_webhook),
            )
            .route(
                "/api/skills/getWalletHoldings",
                post(solana_skills::get_wallet_holdings),
            )
            .route(
                "/api/skills/getMarketData",
                post(solana_skills::get_market_data),
            )
            .route(
                "/api/skills/getTokenMetadata",
                post(solana_skills::get_token_metadata),
            )
            .route(
                "/api/skills/getLiquidity",
                post(solana_skills::get_liquidity),
            )
            .route(
                "/api/skills/getProtocolEvents",
                post(solana_skills::get_protocol_events),
            )
            .fallback_service(ServeDir::new(web_root).not_found_service(ServeFile::new(index)))
            .layer(TraceLayer::new_for_http())
            .with_state(service);
        let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let port = std::env::var("PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(4173);
        let listener = tokio::net::TcpListener::bind((host.as_str(), port))
            .await
            .map_err(|e| e.to_string())?;
        println!("Reconsile running at http://{host}:{port}");
        axum::serve(listener, app).await.map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schedule_disabled_is_not_due() {
        let check = Check {
            schedule_config: Some(ScheduleConfig {
                frequency: "daily".into(),
                time: "00:00".into(),
                weekday: "1".into(),
                timezone: "UTC".into(),
                enabled: false,
            }),
            ..Default::default()
        };
        assert_eq!(schedule_slot(&check), None)
    }

    #[tokio::test]
    async fn inbound_history_is_deduplicated_and_keeps_the_reply() {
        let path = std::env::temp_dir().join(format!(
            "reconsile-inbound-history-{}.json",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let service = AppService {
            store: Store::open_path(path.clone()).await.unwrap(),
            active_runs: Arc::new(StdMutex::new(HashSet::new())),
            run_cancellations: Arc::new(StdMutex::new(HashMap::new())),
            inbound_context: Arc::new(StdMutex::new(HashMap::new())),
        };
        service
            .log_inbound_message("whatsapp", "wamid.test", "15551234567", "Hello")
            .await
            .unwrap();
        service
            .log_inbound_message("whatsapp", "wamid.test", "15551234567", "Hello")
            .await
            .unwrap();
        service
            .append_inbound_log(
                "whatsapp",
                "wamid.test",
                "reply",
                "Hi! How can I help?",
                "complete",
            )
            .await
            .unwrap();

        let state = service.state().await;
        let inbound: Vec<_> = state
            .runs
            .iter()
            .filter(|run| run.mode.as_deref() == Some("whatsapp-inbound"))
            .collect();
        assert_eq!(inbound.len(), 1);
        assert_eq!(inbound[0].logs.len(), 2);
        assert!(inbound[0].logs[1].message.contains("How can I help"));
        drop(state);
        let _ = tokio::fs::remove_file(path).await;
    }
}
