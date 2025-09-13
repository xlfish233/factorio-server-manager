use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde_json::Value;

use crate::error::{AppError, AppResult};

// Same routes, intended to be nested under "/api".
pub fn api_router() -> Router<crate::state::AppState> {
    Router::new()
        .route("/log/tail", get(log_tail))
        .route("/config", get(load_config))
}

#[derive(serde::Deserialize)]
struct TailParams {
    lines: Option<usize>,
}

async fn log_tail(
    State(state): State<crate::state::AppState>,
    Query(p): Query<TailParams>,
) -> AppResult<Json<Vec<String>>> {
    let path = &state.config.log_file;
    let content = tokio::fs::read_to_string(path).await.unwrap_or_default();
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    if let Some(n) = p.lines {
        if lines.len() > n {
            lines = lines.split_off(lines.len() - n);
        }
    }
    Ok(Json(lines))
}

async fn load_config(State(state): State<crate::state::AppState>) -> AppResult<Json<Value>> {
    let cfg_path = &state.config.config_file;
    let content = tokio::fs::read_to_string(cfg_path)
        .await
        .map_err(|e| AppError::Config {
            msg: format!("Could not retrieve config.ini: {}", e),
        })?;
    let mut root = serde_json::Map::new();
    let mut current = String::from("");
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current = line.trim_matches(&['[', ']'][..]).to_string();
            root.entry(current.clone())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
        } else if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim();
            let sec = root
                .entry(current.clone())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            if let Value::Object(map) = sec {
                map.insert(k.to_string(), Value::String(v.to_string()));
            }
        }
    }
    Ok(Json(Value::Object(root)))
}
