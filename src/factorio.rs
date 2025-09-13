use regex::Regex;
use serde_json::Value;
use std::{fs, path::PathBuf};
use tokio::process::Command;

use crate::{
    config::Config,
    error::AppError,
    state::{AppState, ServerState},
};
use tracing::{debug, warn};

pub async fn init(state: &AppState) -> Result<(), AppError> {
    // Ensure saves directory exists early to avoid 500s later
    if let Err(e) = tokio::fs::create_dir_all(&state.config.saves_dir).await {
        return Err(AppError::Config {
            msg: format!(
                "failed to create saves_dir '{}': {}",
                state.config.saves_dir, e
            ),
        });
    }
    debug!(dir = %state.config.saves_dir, "saves directory ensured");

    ensure_settings_file(&state.config).map_err(|e| AppError::Config { msg: e })?;
    // Load settings JSON
    let settings_json = tokio::fs::read_to_string(&state.config.settings_file)
        .await
        .unwrap_or_else(|_| "{}".into());
    let settings: Value =
        serde_json::from_str(&settings_json).unwrap_or(Value::Object(serde_json::Map::new()));
    {
        let mut s = state.server.write().await;
        s.settings = settings;
    }

    // Load Factorio version via --version
    if let Ok(ver) = load_version(&state.config).await {
        let mut s = state.server.write().await;
        s.fac_version = ver;
    }
    // Load base mod version from info.json
    if let Ok(base_ver) = load_base_mod_version(&state.config).await {
        let mut s = state.server.write().await;
        s.base_mod_version = base_ver;
    }

    // Ensure admins file exists; if present, read `admins` into settings
    if let Ok(true) = tokio::fs::try_exists(&state.config.admin_file).await {
        if let Ok(admins) = tokio::fs::read_to_string(&state.config.admin_file).await {
            if let Ok(json) = serde_json::from_str::<Value>(&admins) {
                let mut s = state.server.write().await;
                if let Value::Object(map) = &mut s.settings {
                    map.insert("admins".into(), json);
                }
            }
        }
    } else {
        let _ = tokio::fs::write(&state.config.admin_file, "[]").await;
    }
    Ok(())
}

fn ensure_settings_file(cfg: &Config) -> Result<(), String> {
    // Ensure config dir exists
    if let Some(dir) = PathBuf::from(&cfg.settings_file).parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    if !PathBuf::from(&cfg.settings_file).exists() {
        // Try to copy example from factorio_dir/data/server-settings.example.json
        let example = PathBuf::from(&cfg.factorio_dir)
            .join("data")
            .join("server-settings.example.json");
        match fs::read(&example) {
            Ok(content) => {
                fs::write(&cfg.settings_file, content)
                    .map_err(|e| format!("failed to create server settings file: {}", e))?;
            }
            Err(read_err) => {
                // Fallback: write built-in default compatible with Go implementation
                warn!(
                    "server-settings.example.json not found at {:?} ({}). Using built-in defaults.",
                    example, read_err
                );
                fs::write(&cfg.settings_file, DEFAULT_SERVER_SETTINGS.as_bytes())
                    .map_err(|e| format!("failed to create default server settings file: {}", e))?;
            }
        }
    }
    Ok(())
}

// Default server settings JSON used when Factorio example is unavailable.
// Mirrors the structure and comments of the official example.
const DEFAULT_SERVER_SETTINGS: &str = r#"{
    "_comment_afk_autokick_interval": "How many minutes until someone is kicked when doing nothing, 0 for never.",
    "_comment_allow_commands": "possible values are, true, false and admins-only",
    "_comment_auto_pause": "Whether should the server be paused when no players are present.",
    "_comment_auto_pause_when_players_connect": "Whether should the server be paused when someone is connecting to the server.",
    "_comment_autosave_interval": "Autosave interval in minutes",
    "_comment_autosave_only_on_server": "Whether autosaves should be saved only on server or also on all connected clients. Default is true.",
    "_comment_autosave_slots": "server autosave slots, it is cycled through when the server autosaves.",
    "_comment_credentials": "Your factorio.com login credentials. Required for games with visibility public",
    "_comment_ignore_player_limit_for_returning_players": "Players that played on this map already can join even when the max player limit was reached.",
    "_comment_max_heartbeats_per_second": "Network tick rate. Maximum rate game updates packets are sent at before bundling them together. Minimum value is 6, maximum value is 240.",
    "_comment_max_players": "Maximum number of players allowed, admins can join even a full server. 0 means unlimited.",
    "_comment_max_upload_in_kilobytes_per_second": "optional, default value is 0. 0 means unlimited.",
    "_comment_max_upload_slots": "optional, default value is 5. 0 means unlimited.",
    "_comment_minimum_latency_in_ticks": "optional one tick is 16ms in default speed, default value is 0. 0 means no minimum.",
    "_comment_non_blocking_saving": "Highly experimental feature, enable only at your own risk of losing your saves. On UNIX systems, server will fork itself to create an autosave. Autosaving on connected Windows clients will be disabled regardless of autosave_only_on_server option.",
    "_comment_require_user_verification": "When set to true, the server will only allow clients that have a valid Factorio.com account",
    "_comment_segment_sizes": "Long network messages are split into segments that are sent over multiple ticks. Their size depends on the number of peers currently connected. Increasing the segment size will increase upload bandwidth requirement for the server and download bandwidth requirement for clients. This setting only affects server outbound messages. Changing these settings can have a negative impact on connection stability for some clients.",
    "_comment_token": "Authentication token. May be used instead of 'password' above.",
    "_comment_visibility": [
        "public: Game will be published on the official Factorio matching server",
        "lan: Game will be broadcast on LAN"
    ],
    "admins": [],
    "afk_autokick_interval": 0,
    "allow_commands": "admins-only",
    "auto_pause": true,
    "auto_pause_when_players_connect": false,
    "autosave_interval": 10,
    "autosave_only_on_server": true,
    "autosave_slots": 5,
    "description": "Description of the game that will appear in the listing",
    "game_password": "",
    "ignore_player_limit_for_returning_players": false,
    "max_heartbeats_per_second": 60,
    "max_players": 0,
    "max_upload_in_kilobytes_per_second": 0,
    "max_upload_slots": 5,
    "maximum_segment_size": 100,
    "maximum_segment_size_peer_count": 10,
    "minimum_latency_in_ticks": 0,
    "minimum_segment_size": 25,
    "minimum_segment_size_peer_count": 20,
    "name": "Name of the game as it will appear in the game listing",
    "non_blocking_saving": false,
    "only_admins_can_pause_the_game": true,
    "password": "",
    "require_user_verification": true,
    "tags": [
        "game",
        "tags"
    ],
    "token": "",
    "username": "",
    "visibility": {
        "lan": true,
        "public": true
    }
}"#;

async fn load_version(cfg: &Config) -> Result<[u32; 4], AppError> {
    let out = Command::new(&cfg.factorio_binary)
        .arg("--version")
        .output()
        .await
        .map_err(|e| AppError::Config {
            msg: format!("error loading factorio version: {}", e),
        })?;
    let s = String::from_utf8_lossy(&out.stdout);
    let re = Regex::new(r"Version.*?((\d+\.)?(\d+\.)?(\*|\d+)+)").unwrap();
    if let Some(caps) = re.captures(&s) {
        if let Some(m) = caps.get(1) {
            let parts: Vec<u32> = m
                .as_str()
                .split('.')
                .filter_map(|p| p.parse().ok())
                .collect();
            let mut arr = [0u32; 4];
            let len = parts.len().min(4);
            arr[..len].copy_from_slice(&parts[..len]);
            return Ok(arr);
        }
    }
    Err(AppError::Config {
        msg: "could not parse factorio --version".into(),
    })
}

async fn load_base_mod_version(cfg: &Config) -> Result<String, AppError> {
    let path = PathBuf::from(&cfg.factorio_dir)
        .join(&cfg.factorio_base_mod_dir)
        .join("info.json");
    let data = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| AppError::Config {
            msg: format!("couldn't open baseMods info.json: {}", e),
        })?;
    let v: Value = serde_json::from_str(&data).map_err(|e| AppError::Config {
        msg: format!("error unmarshalling baseMods info.json: {}", e),
    })?;
    Ok(v.get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string())
}

pub async fn create_save(cfg: &Config, file_path: &str) -> Result<String, AppError> {
    if let Some(dir) = PathBuf::from(file_path).parent() {
        tokio::fs::create_dir_all(dir).await.ok();
    }
    tracing::info!(binary=%cfg.factorio_binary, out_path=%file_path, "creating factorio save");
    let out = Command::new(&cfg.factorio_binary)
        .arg("--create")
        .arg(file_path)
        .output()
        .await
        .map_err(|e| AppError::Config {
            msg: format!("error creating factorio save: {}", e),
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        tracing::error!(binary=%cfg.factorio_binary, code=?out.status.code(), stderr=%stderr, "factorio --create failed");
        return Err(AppError::Config {
            msg: format!(
                "create save failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into())
}

pub async fn build_start_args(cfg: &Config, s: &ServerState) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--bind".into(),
        s.bindip.clone(),
        "--port".into(),
        s.port.to_string(),
        "--server-settings".into(),
        cfg.settings_file.clone(),
        "--rcon-port".into(),
        cfg.factorio_rcon_port.to_string(),
        "--rcon-password".into(),
        cfg.factorio_rcon_pass.clone(),
    ];
    // admin list仅在 >= 0.17 才支持；低版本传该参数会直接退出
    let ge_017 = (s.fac_version[0] > 0) || (s.fac_version[0] == 0 && s.fac_version[1] >= 17);
    if ge_017 && PathBuf::from(&cfg.admin_file).exists() {
        args.push("--server-adminlist".into());
        args.push(cfg.admin_file.clone());
    }
    if s.savefile.starts_with("Load Latest") {
        args.push("--start-server-load-latest".into());
    } else {
        args.push("--start-server".into());
        args.push(
            PathBuf::from(&cfg.saves_dir)
                .join(&s.savefile)
                .to_string_lossy()
                .into(),
        );
    }
    if !cfg.chat_log_file.is_empty() {
        args.push("--console-log".into());
        args.push(cfg.chat_log_file.clone());
    }
    args
}

pub async fn ensure_rcon_connected(state: &AppState) -> Result<(), AppError> {
    let port = state.config.factorio_rcon_port;
    if port == 0 {
        return Err(AppError::BadRequest {
            msg: "RCON port not configured".into(),
        });
    }
    if state.rcon_conn.lock().await.is_some() {
        return Ok(());
    }
    let ip = { state.server.read().await.bindip.clone() };
    let addr = format!("{}:{}", ip, port);
    let pass = state.config.factorio_rcon_pass.clone();
    let conn = rcon::Connection::builder()
        .enable_factorio_quirks(true)
        .connect(addr, &pass)
        .await
        .map_err(|e| AppError::Config {
            msg: format!("rcon connect failed: {}", e),
        })?;
    *state.rcon_conn.lock().await = Some(conn);
    Ok(())
}

pub async fn rcon_send(state: &AppState, cmd: &str) -> Result<String, AppError> {
    if state.rcon_conn.lock().await.is_none() {
        let _ = ensure_rcon_connected(state).await;
    }
    let mut guard = state.rcon_conn.lock().await;
    match guard.as_mut() {
        Some(conn) => {
            let resp = conn.cmd(cmd).await.map_err(|e| AppError::Config {
                msg: format!("rcon cmd failed: {}", e),
            })?;
            Ok(resp)
        }
        None => Err(AppError::BadRequest {
            msg: "RCON not connected".into(),
        }),
    }
}
