mod config;
mod db;
mod entity;
mod error;
mod extractors;
mod factorio;
mod middleware;
mod routes;
mod save_parser;
mod services;
mod sessions;
mod state;

use crate::config::persist_generated;
use crate::config::Config;
use crate::routes::build_router;
use crate::routes::server::start_process_with_current_settings;
use crate::state::AppState;
use axum::extract::DefaultBodyLimit;
use axum_extra::extract::cookie::Key;
use base64::Engine;
use sha2::{Digest, Sha512};
use tower_http::trace::TraceLayer;
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize structured logging via tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info,axum=info")),
        )
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .compact()
        .init();

    // Load configuration from TOML + env (Figment).
    let mut cfg = Config::load();
    debug!(?cfg.bind_addr, secure=?cfg.secure, "Configuration loaded");

    // Initialize database connection (SQLite for now).
    let db = db::connect(&cfg).await?;

    // Prepare cookie signing key (persist if generated)
    let mut persist_cookie_bytes: Option<Vec<u8>> = None;
    let cookie_key = if cfg.cookie_key_b64.is_empty() {
        use rand::RngCore;
        let mut bytes = [0u8; 64];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        persist_cookie_bytes = Some(bytes.to_vec());
        Key::from(&bytes)
    } else {
        match base64::engine::general_purpose::STANDARD.decode(cfg.cookie_key_b64.as_bytes()) {
            Ok(mut bytes) => {
                let key_bytes = if bytes.len() != 64 {
                    let digest = Sha512::digest(&bytes);
                    digest.to_vec()
                } else {
                    std::mem::take(&mut bytes)
                };
                Key::from(&key_bytes)
            }
            Err(e) => {
                warn!(error=%e, "Invalid base64 in FSMR_COOKIE_KEY; generating development key");
                use rand::RngCore;
                let mut bytes = [0u8; 64];
                rand::rngs::OsRng.fill_bytes(&mut bytes);
                persist_cookie_bytes = Some(bytes.to_vec());
                Key::from(&bytes)
            }
        }
    };

    // Generate and persist rcon_pass/port if missing
    let rcon_port_was_zero = cfg.factorio_rcon_port == 0;
    if cfg.factorio_rcon_pass.is_empty() {
        use rand::distributions::{Alphanumeric, DistString};
        let pass = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
        cfg.factorio_rcon_pass = pass;
    }

    // Shared application state for handlers.
    let sessions = std::sync::Arc::new(dashmap::DashMap::new());
    let server = std::sync::Arc::new(tokio::sync::RwLock::new(state::ServerState {
        savefile: String::from("Load Latest"),
        latency: 0,
        bindip: "0.0.0.0".into(),
        port: 34197,
        running: false,
        fac_version: [1, 1, 6, 0],
        base_mod_version: "1.1.6".into(),
        settings: serde_json::json!({}),
    }));
    let (server_status_tx, _rx) = tokio::sync::broadcast::channel(64);
    let (gamelog_tx, _rx2) = tokio::sync::broadcast::channel(256);
    let state = AppState {
        db,
        config: cfg.clone(),
        sessions,
        cookie_key,
        server,
        server_status_tx,
        server_process: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
        gamelog_tx,
        console_cache: std::sync::Arc::new(tokio::sync::Mutex::new(
            std::collections::VecDeque::with_capacity(cfg.console_cache_size),
        )),
        rcon_conn: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
        mods_lock: std::sync::Arc::new(tokio::sync::RwLock::new(())),
    };

    // Initialize Factorio-related state (settings/version/base mod)
    if let Err(e) = factorio::init(&state).await {
        warn!(error=%e, "Factorio init warning");
    }
    // Persist generated values (best-effort)
    persist_generated(
        &cfg,
        persist_cookie_bytes.as_deref(),
        if cfg.factorio_rcon_pass.is_empty() {
            None
        } else {
            Some(&cfg.factorio_rcon_pass)
        },
        rcon_port_was_zero,
        false,
    );
    // Autostart if configured
    if cfg.autostart {
        let st = state.clone();
        tokio::spawn(async move {
            // Ensure there's a savefile value (default already set to "Load Latest")
            if let Err(e) = start_process_with_current_settings(st.clone()).await {
                warn!(error=%e, "Autostart failed");
            } else {
                info!("Autostarted Factorio server");
            }
        });
    }

    // Build router tree.
    let app = build_router(state.clone())
        // Log all HTTP requests/responses (including WS handshakes)
        .layer(TraceLayer::new_for_http())
        // Limit request body globally (uploads)
        .layer(DefaultBodyLimit::max(cfg.max_upload_bytes));

    // Bind and serve.
    let listener = tokio::net::TcpListener::bind(&cfg.bind_addr).await?;
    info!(addr=%cfg.bind_addr, "HTTP listening");
    axum::serve(listener, app).await?;
    Ok(())
}
