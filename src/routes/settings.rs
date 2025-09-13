use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};

use crate::{
    error::{AppError, AppResult},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings", get(get_settings))
        .route("/settings/update", post(update_settings))
}

async fn get_settings(State(state): State<AppState>) -> AppResult<Json<serde_json::Value>> {
    let s = state.server.read().await;
    Ok(Json(s.settings.clone()))
}

async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<String> {
    // Update in-memory
    {
        let mut s = state.server.write().await;
        s.settings = body.clone();
    }
    // Write server-settings.json
    let settings_json = serde_json::to_vec_pretty(&body).map_err(|e| AppError::Config {
        msg: format!("Failed to marshal server settings: {}", e),
    })?;
    tokio::fs::write(&state.config.settings_file, settings_json)
        .await
        .map_err(|e| AppError::Config {
            msg: format!("Failed to save server settings: {}", e),
        })?;

    // Only write admin list if Factorio version >= 0.17.x
    if let Some(admins) = body.get("admins") {
        let v = state.server.read().await.fac_version;
        let ge_017 = (v[0] > 0) || (v[0] == 0 && v[1] >= 17);
        if ge_017 {
            let admins_json = serde_json::to_vec_pretty(admins).map_err(|e| AppError::Config {
                msg: format!("Failed to marshal admins-Setting: {}", e),
            })?;
            tokio::fs::write(&state.config.admin_file, admins_json)
                .await
                .map_err(|e| AppError::Config {
                    msg: format!("Failed to save admins: {}", e),
                })?;
        }
    }

    Ok("Settings successfully saved".into())
}
