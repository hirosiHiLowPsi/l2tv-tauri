use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::{Mutex, Semaphore};

use crate::database;
use crate::error::{Result, message};
use crate::tables;

pub struct CacheEntry {
    pub fetched_at: Instant,
    pub value: Value,
}

#[derive(Clone, Debug, Default)]
pub struct BeatorajaCalendarSource {
    pub score_db_path: String,
    pub song_db_path: String,
    pub language: String,
    pub theme: String,
}

pub struct AppState {
    pub client: reqwest::Client,
    pub stellaverse_gate: Semaphore,
    pub last_stellaverse_request: Mutex<Option<Instant>>,
    pub table_cache: Mutex<HashMap<String, CacheEntry>>,
    pub stellaverse_cache: Mutex<HashMap<String, CacheEntry>>,
    pub beatoraja_calendar_source: Mutex<Option<BeatorajaCalendarSource>>,
    pub e2e_marker_path: Option<PathBuf>,
}

impl AppState {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("L2TV/2.1 Tauri")
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(25))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("HTTP client construction failed");
        Self {
            client,
            stellaverse_gate: Semaphore::new(1),
            last_stellaverse_request: Mutex::new(None),
            table_cache: Mutex::new(HashMap::new()),
            stellaverse_cache: Mutex::new(HashMap::new()),
            beatoraja_calendar_source: Mutex::new(None),
            e2e_marker_path: if cfg!(debug_assertions) {
                std::env::var_os("L2TV_E2E_MARKER").map(PathBuf::from)
            } else {
                None
            },
        }
    }
}

pub async fn request(path: &str, body: Value, state: &AppState) -> Result<Value> {
    match path {
        "/api/analyze" => database::analyze(body, state).await,
        "/api/table-list" => tables::table_list(body, state).await,
        "/api/table-meta" => tables::table_meta(body, state).await,
        "/api/profile-from-db" => database::profile_from_db(body, state).await,
        "/api/local-db-state" => database::local_db_state(body).await,
        "/api/beatoraja-history" => database::beatoraja_history(body).await,
        _ => Err(message("未対応のローカルAPIです。")),
    }
}

pub fn trim_cache(cache: &mut HashMap<String, CacheEntry>, max_entries: usize) {
    while cache.len() > max_entries {
        let oldest = cache
            .iter()
            .min_by_key(|(_, entry)| entry.fetched_at)
            .map(|(key, _)| key.clone());
        if let Some(key) = oldest {
            cache.remove(&key);
        } else {
            break;
        }
    }
}

pub type SharedState<'a> = tauri::State<'a, AppState>;
