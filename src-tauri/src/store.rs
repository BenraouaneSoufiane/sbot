use crate::models::*;
use chrono::Utc;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Store {
    path: PathBuf,
    pub data: Arc<Mutex<AppData>>,
}

impl Store {
    pub async fn open() -> Result<Self, String> {
        let dir = std::env::var_os("RECONSILE_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".data"));
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| e.to_string())?;
        Self::open_path(dir.join("state.json")).await
    }

    pub async fn open_path(path: PathBuf) -> Result<Self, String> {
        let data = match tokio::fs::read_to_string(&path).await {
            Ok(raw) => {
                serde_json::from_str(&raw).map_err(|e| format!("Invalid state file: {e}"))?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => initial_state(),
            Err(error) => return Err(error.to_string()),
        };
        let store = Self {
            path,
            data: Arc::new(Mutex::new(data)),
        };
        Ok(store)
    }

    pub async fn save_data(&self, data: &AppData) -> Result<(), String> {
        save_to(&self.path, data).await
    }

    pub async fn recover_interrupted_runs(&self) -> Result<(), String> {
        let snapshot = {
            let mut data = self.data.lock().await;
            let interrupted_checks = data
                .runs
                .iter_mut()
                .filter(|run| run.status == "running")
                .map(|run| {
                    run.status = "failed".into();
                    run.duration = "Interrupted".into();
                    run.logs.push(RunLog {
                        id: format!("log-{}-{}", Utc::now().timestamp_millis(), run.logs.len()),
                        step: "run".into(),
                        message: "Run was interrupted by a service restart".into(),
                        status: "failed".into(),
                        timestamp: Utc::now().to_rfc3339(),
                    });
                    run.check_id.clone()
                })
                .collect::<Vec<_>>();
            for check_id in interrupted_checks {
                if let Some(check) = data.checks.iter_mut().find(|check| check.id == check_id) {
                    check.status = "failed".into();
                }
            }
            data.clone()
        };
        self.save_data(&snapshot).await
    }
}

async fn save_to(path: &Path, data: &AppData) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(data).map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    tokio::fs::write(&temp, bytes)
        .await
        .map_err(|e| e.to_string())?;
    tokio::fs::rename(temp, path)
        .await
        .map_err(|e| e.to_string())
}

fn initial_state() -> AppData {
    serde_json::from_value(serde_json::json!({
      "connections": {},
      "checks": [
        {"id":"stripe-settlement","name":"Stripe settlement","description":"Orders, payouts and bank deposits","status":"attention","schedule":"Every day · 09:00","scheduleConfig":{"frequency":"daily","time":"09:00","weekday":"1","timezone":"UTC","enabled":true},"lastRun":"Today, 09:02","matchRate":98.7,"prompt":"Match each Stripe charge to an order by order_id. Confirm settled charges appear in the bank feed within 2 business days. Flag duplicates, refunds without a matching charge, and differences above $1.00.","sources":[{"id":"src-orders","name":"Store orders","type":"api","url":"demo://orders","auth":"bearer","token":""},{"id":"src-payouts","name":"Stripe payouts","type":"api","url":"demo://payouts","auth":"bearer","token":""}],"notifications":[{"id":"notif-email","type":"email","label":"Finance inbox","recipient":"finance@acme.test","enabled":true}]},
        {"id":"inventory-count","name":"Warehouse inventory","description":"System quantities vs physical count","status":"healthy","schedule":"Fridays · 17:00","scheduleConfig":{"frequency":"weekly","time":"17:00","weekday":"5","timezone":"UTC","enabled":true},"lastRun":"Aug 1, 17:04","matchRate":100.0,"prompt":"Compare physical_count against system_quantity by sku and flag any non-zero variance.","sources":[{"id":"src-stock","name":"Stock count","type":"url","url":"demo://inventory","auth":"none","token":""}],"notifications":[]}
      ],
      "runs": [
        {"id":"run-1042","checkId":"stripe-settlement","check":"Stripe settlement","status":"attention","startedAt":"Today, 09:02","duration":"42s","records":1248,"matched":1232,"exceptions":16,"amount":"$2,481.20"},
        {"id":"run-1041","checkId":"inventory-count","check":"Warehouse inventory","status":"complete","startedAt":"Aug 1, 17:04","duration":"18s","records":386,"matched":386,"exceptions":0,"amount":"$0.00"},
        {"id":"run-1040","checkId":"stripe-settlement","check":"Stripe settlement","status":"complete","startedAt":"Aug 1, 09:01","duration":"39s","records":1190,"matched":1190,"exceptions":0,"amount":"$0.00"}
      ],
      "exceptions": [
        {"id":"EX-281","checkId":"stripe-settlement","title":"Payout missing from bank feed","detail":"Stripe PO-8841 · expected Jul 31","amount":"$1,842.00","severity":"high","age":"4 days","owner":"Unassigned"},
        {"id":"EX-280","checkId":"stripe-settlement","title":"Order total differs from charge","detail":"Order #10492 · Stripe ch_3Q92…","amount":"$428.20","severity":"medium","age":"2 days","owner":"Maya"},
        {"id":"EX-279","checkId":"stripe-settlement","title":"Duplicate charge detected","detail":"Order #10501 · two charges within 4s","amount":"$211.00","severity":"medium","age":"1 day","owner":"Unassigned"}
      ]
    }))
    .expect("valid initial state")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn recovers_runs_interrupted_by_restart() {
        let path = std::env::temp_dir().join(format!(
            "reconsile-recovery-{}.json",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let store = Store::open_path(path.clone()).await.unwrap();
        {
            let mut data = store.data.lock().await;
            data.runs[0].status = "running".into();
            data.checks[0].status = "running".into();
        }

        store.recover_interrupted_runs().await.unwrap();

        let data = store.data.lock().await;
        assert_eq!(data.runs[0].status, "failed");
        assert_eq!(data.runs[0].duration, "Interrupted");
        assert_eq!(data.checks[0].status, "failed");
        assert!(data.runs[0]
            .logs
            .last()
            .unwrap()
            .message
            .contains("service restart"));
        drop(data);
        let _ = tokio::fs::remove_file(path).await;
    }
}
