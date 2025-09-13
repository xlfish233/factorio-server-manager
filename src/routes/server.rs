use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use crate::{
    error::{AppError, AppResult},
    factorio,
    state::{AppState, ServerState},
};

#[derive(Deserialize)]
struct StartReq {
    savefile: Option<String>,
    bindip: Option<String>,
    port: Option<u16>,
    latency: Option<i32>,
}

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/server/start",
            post(start_server).route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                crate::middleware::server_off,
            )),
        )
        .route("/server/stop", get(stop_server))
        .route("/server/kill", get(kill_server))
        .route("/server/status", get(server_status))
        .route("/server/facVersion", get(fac_version))
}

async fn start_server(
    State(state): State<AppState>,
    Json(req): Json<StartReq>,
) -> AppResult<String> {
    let mut s = state.server.write().await;
    if s.running {
        return Err(AppError::Conflict {
            msg: "Factorio server is already running".into(),
        });
    }
    if let Some(sf) = req.savefile {
        s.savefile = sf;
    }
    if let Some(ip) = req.bindip {
        s.bindip = ip;
    }
    if let Some(p) = req.port {
        s.port = p;
    }
    if let Some(l) = req.latency {
        s.latency = l;
    }
    if s.savefile.trim().is_empty() {
        return Err(AppError::BadRequest {
            msg: "no savefile provided".into(),
        });
    }
    // Guard against selecting Factorio's temporary save files (e.g., *.tmp.zip)
    if s.savefile.ends_with(".tmp.zip") {
        let candidate = s.savefile.trim_end_matches(".tmp.zip").to_string() + ".zip";
        let real = std::path::Path::new(&state.config.saves_dir).join(&candidate);
        if real.exists() {
            tracing::warn!(selected=%s.savefile, corrected=%candidate, "selected temp save; switching to final .zip");
            s.savefile = candidate;
        } else {
            tracing::warn!(selected=%s.savefile, "selected temp save; no final .zip found");
        }
    }
    drop(s);
    // spawn Factorio process with args
    start_process_with_current_settings(state.clone()).await?;
    // Poll up to 3 seconds to confirm process stays up
    use tokio::time::{sleep, Duration};
    for attempt in 1..=3u8 {
        sleep(Duration::from_secs(1)).await;
        let exited = {
            let mut guard = state.server_process.lock().await;
            match guard.as_mut() {
                Some(child) => match child.try_wait() {
                    Ok(Some(_status)) => true,
                    Ok(None) => false,
                    Err(e) => {
                        tracing::warn!(error=%e, "try_wait failed during start poll");
                        false
                    }
                },
                None => true,
            }
        };
        if exited {
            tracing::error!(attempt, "Factorio process exited shortly after start");
            return Err(AppError::Config {
                msg: "Factorio failed to start (exited)".into(),
            });
        }
    }
    let s = state.server.read().await;
    Ok(format!("Factorio server with save: {} started", s.savefile))
}

pub async fn start_process_with_current_settings(state: AppState) -> AppResult<()> {
    // Preconditions: caller has validated and set ServerState fields.
    let s_read = state.server.read().await.clone();
    let args = factorio::build_start_args(&state.config, &s_read).await;
    tracing::info!(
        savefile=%s_read.savefile,
        bindip=%s_read.bindip,
        port=%s_read.port,
        binary=%state.config.factorio_binary,
        args=%format!("{:?}", args),
        cwd=%std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default(),
        "starting Factorio process"
    );
    let mut cmd = tokio::process::Command::new(&state.config.factorio_binary);
    cmd.args(args);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| AppError::Config {
        msg: format!("Factorio process failed to start: {}", e),
    })?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let log_path = state.config.console_log_file.clone();
    let gamelog_tx = state.gamelog_tx.clone();
    let cache = state.console_cache.clone();
    let cache_size = state.config.console_cache_size;
    let state_for_stdout = state.clone();
    tokio::spawn(async move {
        if let Some(mut out) = stdout {
            pipe_and_log(
                &mut out,
                &log_path,
                gamelog_tx.clone(),
                cache.clone(),
                cache_size,
                state_for_stdout.clone(),
            )
            .await;
        }
    });
    let log_path2 = state.config.console_log_file.clone();
    let gamelog_tx2 = state.gamelog_tx.clone();
    let cache2 = state.console_cache.clone();
    let cache_size2 = state.config.console_cache_size;
    let state_for_stderr = state.clone();
    tokio::spawn(async move {
        if let Some(mut err) = stderr {
            pipe_and_log(
                &mut err,
                &log_path2,
                gamelog_tx2.clone(),
                cache2.clone(),
                cache_size2,
                state_for_stderr.clone(),
            )
            .await;
        }
    });
    {
        let mut proc_slot = state.server_process.lock().await;
        *proc_slot = Some(child);
    }
    {
        let mut s = state.server.write().await;
        s.running = true;
    }
    let payload =
        serde_json::to_string(&*state.server.read().await).unwrap_or_else(|_| "{}".into());
    tracing::info!(len=%payload.len(), "broadcasting server_status=running true after start");
    let _ = state.server_status_tx.send(payload);
    // Spawn a lightweight monitor that polls child.try_wait() and broadcasts when the process exits.
    let monitor_state = state.clone();
    tokio::spawn(async move {
        use tokio::time::{sleep, Duration};
        loop {
            let exited = {
                let mut guard = monitor_state.server_process.lock().await;
                match guard.as_mut() {
                    Some(child) => match child.try_wait() {
                        Ok(Some(_status)) => {
                            *guard = None;
                            true
                        }
                        Ok(None) => false,
                        Err(e) => {
                            tracing::warn!(error=%e, "try_wait on Factorio process failed");
                            false
                        }
                    },
                    None => {
                        return;
                    }
                }
            };
            if exited {
                // Update running=false and clear RCON
                {
                    let mut s = monitor_state.server.write().await;
                    s.running = false;
                }
                *monitor_state.rcon_conn.lock().await = None;
                let payload = serde_json::to_string(&*monitor_state.server.read().await)
                    .unwrap_or_else(|_| "{}".into());
                tracing::info!(len=%payload.len(), "broadcasting server_status=running false after exit");
                let _ = monitor_state.server_status_tx.send(payload);
                break;
            }
            sleep(Duration::from_secs(1)).await;
        }
    });
    Ok(())
}

async fn stop_server(State(state): State<AppState>) -> AppResult<String> {
    let mut s = state.server.write().await;
    if !s.running {
        return Err(AppError::Conflict {
            msg: "Factorio server is not running".into(),
        });
    }
    // try graceful stop
    if let Some(child) = state.server_process.lock().await.as_mut() {
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal::SIGINT};
            use nix::unistd::Pid;
            let _ = kill(Pid::from_raw(child.id().unwrap_or(0) as i32), SIGINT);
        }
    }
    s.running = false;
    // drop rcon connection
    *state.rcon_conn.lock().await = None;
    let payload = serde_json::to_string(&*s).unwrap_or_else(|_| "{}".into());
    let _ = state.server_status_tx.send(payload);
    Ok("Factorio server stopped".into())
}

async fn kill_server(State(state): State<AppState>) -> AppResult<String> {
    let mut s = state.server.write().await;
    if !s.running {
        return Err(AppError::BadRequest {
            msg: "Factorio server is not running".into(),
        });
    }
    if let Some(child) = state.server_process.lock().await.as_mut() {
        let _ = child.start_kill();
    }
    s.running = false;
    *state.rcon_conn.lock().await = None;
    let payload = serde_json::to_string(&*s).unwrap_or_else(|_| "{}".into());
    let _ = state.server_status_tx.send(payload);
    Ok("Factorio server killed".into())
}

async fn pipe_and_log<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut R,
    path: &str,
    tx: tokio::sync::broadcast::Sender<String>,
    cache: std::sync::Arc<tokio::sync::Mutex<std::collections::VecDeque<String>>>,
    cache_size: usize,
    state: AppState,
) {
    use tokio::io::AsyncBufReadExt;
    let mut lines = tokio::io::BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        // Write line to log file
        if let Ok(mut f) = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
        {
            use tokio::io::AsyncWriteExt;
            let _ = f.write_all(line.as_bytes()).await;
            let _ = f.write_all(b"\n").await;
        }
        // Update cache and broadcast
        {
            let mut guard = cache.lock().await;
            if guard.len() == cache_size {
                guard.pop_front();
            }
            guard.push_back(line.clone());
        }
        let _ = tx.send(line.clone());
        if line.contains("Starting RCON interface at IP") {
            let _ = crate::factorio::ensure_rcon_connected(&state).await;
        }
    }
}

async fn server_status(State(state): State<AppState>) -> AppResult<Json<ServerState>> {
    let s = state.server.read().await.clone();
    Ok(Json(s))
}

async fn fac_version(State(state): State<AppState>) -> AppResult<Json<serde_json::Value>> {
    let s = state.server.read().await;
    let version = format!(
        "{}.{}.{}.{}",
        s.fac_version[0], s.fac_version[1], s.fac_version[2], s.fac_version[3]
    );
    Ok(Json(
        serde_json::json!({ "version": version, "base_mod_version": s.base_mod_version }),
    ))
}
