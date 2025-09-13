//! Shared application state made available to request handlers.

use crate::config::Config;
use axum_extra::extract::cookie::Key;
use dashmap::DashMap;
use sea_orm::DatabaseConnection;
use std::collections::VecDeque;
use tokio::net::TcpStream;
use tokio::process::Child;
use tokio::sync::{broadcast, Mutex, RwLock};

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Config,
    pub sessions: std::sync::Arc<DashMap<String, String>>, // session_id -> username
    pub cookie_key: Key,
    pub server: std::sync::Arc<tokio::sync::RwLock<ServerState>>, // in-memory server state
    pub server_status_tx: broadcast::Sender<String>,
    pub server_process: std::sync::Arc<tokio::sync::Mutex<Option<Child>>>,
    pub gamelog_tx: broadcast::Sender<String>,
    pub console_cache: std::sync::Arc<Mutex<VecDeque<String>>>,
    pub rcon_conn: std::sync::Arc<Mutex<Option<rcon::Connection<TcpStream>>>>,
    pub mods_lock: std::sync::Arc<RwLock<()>>, // protects operations on config.mods_dir
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct ServerState {
    pub savefile: String,
    pub latency: i32,
    pub bindip: String,
    pub port: u16,
    pub running: bool,
    pub fac_version: [u32; 4],
    pub base_mod_version: String,
    #[serde(skip_serializing)]
    pub settings: serde_json::Value,
}
