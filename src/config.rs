//! Application configuration using Figment (TOML + env).
//! - Default values provide sane local development settings.
//! - Reads from TOML file (default: ./conf.toml or FSMR_CONF path)
//! - Overridden by environment variables with prefix FSMR_

use base64::Engine;
use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Socket address to bind the HTTP server, e.g. "127.0.0.1:3000".
    pub bind_addr: String,
    /// Database connection string (SeaORM/SQLx format). Example: "sqlite://dev.db?mode=rwc".
    pub database_url: String,
    /// Whether to mark cookies as Secure (HTTPS only). Defaults to true.
    pub secure: bool,
    /// Base64-encoded cookie key (32 bytes). Used for signing cookies.
    pub cookie_key_b64: String,
    /// If true, when unauthorized and likely a browser (HTML), redirect to /login (303).
    /// Otherwise respond 401 JSON for API clients.
    pub redirect_on_unauth: bool,
    // Factorio related paths
    pub factorio_dir: String,
    pub saves_dir: String,
    /// Directory where mods (zip and config files) reside.
    pub mods_dir: String,
    pub config_file: String,
    pub log_file: String,
    pub settings_file: String,
    pub admin_file: String,
    /// Where to store Factorio mod portal credentials JSON (username/userkey).
    pub credentials_file: String,
    pub factorio_binary: String,
    pub factorio_config_dir: String,
    pub factorio_base_mod_dir: String,
    pub factorio_rcon_port: u16,
    pub factorio_rcon_pass: String,
    pub console_log_file: String,
    pub chat_log_file: String,
    pub console_cache_size: usize,
    /// Directory to store mod packs.
    pub mod_pack_dir: String,
    /// Max request body size for uploads (bytes).
    pub max_upload_bytes: usize,
    /// Autostart Factorio on service boot.
    pub autostart: bool,
    #[serde(skip)]
    pub conf_path: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:3000".to_string(),
            // Use a file-backed sqlite DB by default for persistence during dev.
            database_url: "sqlite://dev.db?mode=rwc".to_string(),
            secure: true,
            cookie_key_b64: String::new(),
            redirect_on_unauth: true,
            factorio_dir: "./".into(),
            saves_dir: "./saves".into(),
            mods_dir: "./mods".into(),
            config_file: "config/config.ini".into(),
            log_file: "factorio-current.log".into(),
            settings_file: "server-settings.json".into(),
            admin_file: "server-adminlist.json".into(),
            credentials_file: "./factorio.auth".into(),
            factorio_binary: "bin/x64/factorio".into(),
            factorio_config_dir: "config".into(),
            factorio_base_mod_dir: "data/base".into(),
            factorio_rcon_port: 0,
            factorio_rcon_pass: String::new(),
            console_log_file: "factorio-current.log".into(),
            chat_log_file: String::new(),
            console_cache_size: 25,
            mod_pack_dir: "./mod_packs".into(),
            max_upload_bytes: 20 * 1024 * 1024,
            autostart: false,
            conf_path: None,
        }
    }
}

impl Config {
    /// Load configuration via Figment, merging defaults -> TOML -> env.
    /// Env prefix: FSMR_, keys split by '_', e.g. FSMR_BIND_ADDR.
    pub fn load() -> Self {
        let path = std::env::var("FSMR_CONF").unwrap_or_else(|_| "conf.toml".to_string());
        let figment = Figment::from(Serialized::defaults(Self::default()))
            .merge(Toml::file(path))
            .merge(Env::prefixed("FSMR_").split("_"));
        let mut cfg: Self = figment.extract().unwrap_or_else(|e| {
            eprintln!("Config load error, using defaults + env if any: {}", e);
            // As a fallback, just environment variables on top of defaults
            Figment::from(Serialized::defaults(Self::default()))
                .merge(Env::prefixed("FSMR_").split("_"))
                .extract()
                .unwrap_or_default()
        });

        cfg.conf_path =
            Some(std::env::var("FSMR_CONF").unwrap_or_else(|_| "conf.toml".to_string()));
        cfg.normalize_paths();
        // Try auto-detect factorio_dir when binary missing (prefer ../factorio then ./factorio)
        cfg.try_autodetect_factorio_dir();
        cfg.normalize_paths();
        cfg.ensure_rcon_port();
        cfg
    }
}

impl Config {
    fn normalize_paths(&mut self) {
        // factorio_config_dir relative to factorio_dir
        if !Path::new(&self.factorio_config_dir).is_absolute() {
            self.factorio_config_dir = PathBuf::from(&self.factorio_dir)
                .join(&self.factorio_config_dir)
                .to_string_lossy()
                .into();
        }
        // config_file relative to factorio_dir (matches Go behavior)
        if !Path::new(&self.config_file).is_absolute() {
            self.config_file = PathBuf::from(&self.factorio_dir)
                .join(&self.config_file)
                .to_string_lossy()
                .into();
        }
        // settings_file relative to factorio_config_dir
        if !Path::new(&self.settings_file).is_absolute() {
            self.settings_file = PathBuf::from(&self.factorio_config_dir)
                .join(&self.settings_file)
                .to_string_lossy()
                .into();
        }
        // admin_file relative to factorio_config_dir
        if !Path::new(&self.admin_file).is_absolute() {
            self.admin_file = PathBuf::from(&self.factorio_config_dir)
                .join(&self.admin_file)
                .to_string_lossy()
                .into();
        }
        // base mod dir relative to factorio_dir
        if !Path::new(&self.factorio_base_mod_dir).is_absolute() {
            self.factorio_base_mod_dir = PathBuf::from(&self.factorio_dir)
                .join(&self.factorio_base_mod_dir)
                .to_string_lossy()
                .into();
        }
        // saves_dir relative to factorio_dir (Go maps flags to dir/saves)
        if !Path::new(&self.saves_dir).is_absolute() {
            self.saves_dir = PathBuf::from(&self.factorio_dir)
                .join(&self.saves_dir)
                .to_string_lossy()
                .into();
        }
        // mods_dir relative to factorio_dir
        if !Path::new(&self.mods_dir).is_absolute() {
            self.mods_dir = PathBuf::from(&self.factorio_dir)
                .join(&self.mods_dir)
                .to_string_lossy()
                .into();
        }
        // mod_pack_dir relative to factorio_dir
        if !Path::new(&self.mod_pack_dir).is_absolute() {
            self.mod_pack_dir = PathBuf::from(&self.factorio_dir)
                .join(&self.mod_pack_dir)
                .to_string_lossy()
                .into();
        }
        // factorio_binary relative to factorio_dir if not absolute
        if !Path::new(&self.factorio_binary).is_absolute() {
            self.factorio_binary = PathBuf::from(&self.factorio_dir)
                .join(&self.factorio_binary)
                .to_string_lossy()
                .into();
        }
        // console_log_file relative to factorio_dir if not absolute
        if !self.console_log_file.is_empty() && !Path::new(&self.console_log_file).is_absolute() {
            self.console_log_file = PathBuf::from(&self.factorio_dir)
                .join(&self.console_log_file)
                .to_string_lossy()
                .into();
        }
        // chat_log_file relative to factorio_dir if not absolute
        if !self.chat_log_file.is_empty() && !Path::new(&self.chat_log_file).is_absolute() {
            self.chat_log_file = PathBuf::from(&self.factorio_dir)
                .join(&self.chat_log_file)
                .to_string_lossy()
                .into();
        }
        // log_file relative to factorio_dir if not absolute
        if !self.log_file.is_empty() && !Path::new(&self.log_file).is_absolute() {
            self.log_file = PathBuf::from(&self.factorio_dir)
                .join(&self.log_file)
                .to_string_lossy()
                .into();
        }
    }

    fn ensure_rcon_port(&mut self) {
        if self.factorio_rcon_port == 0 {
            // Go picks 40000-45000
            let mut rng = rand::thread_rng();
            self.factorio_rcon_port = rng.gen_range(40000..45000);
        }
    }
}

impl Config {
    fn try_autodetect_factorio_dir(&mut self) {
        let bin = PathBuf::from(&self.factorio_binary);
        if bin.exists() {
            return;
        }
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let candidates = [cwd.join("../factorio"), cwd.join("./factorio")];
        for cand in candidates.iter() {
            let f = cand.join("bin/x64/factorio");
            if f.exists() {
                warn!(old=%self.factorio_dir, new=%cand.display(), "auto-detected factorio_dir");
                self.factorio_dir = cand.to_string_lossy().into();
                return;
            }
        }
    }
}

/// Persist selected generated values back to conf.toml (best-effort).
pub fn persist_generated(
    cfg: &Config,
    gen_cookie_key: Option<&[u8]>,
    gen_rcon_pass: Option<&str>,
    rcon_port_was_zero: bool,
    factorio_dir_changed: bool,
) {
    let Some(path) = cfg.conf_path.clone() else {
        return;
    };
    let mut table = if let Ok(content) = std::fs::read_to_string(&path) {
        toml::from_str::<toml::Table>(&content).unwrap_or_default()
    } else {
        toml::Table::new()
    };

    if let Some(key_bytes) = gen_cookie_key {
        table.insert(
            "cookie_key_b64".into(),
            toml::Value::String(base64::engine::general_purpose::STANDARD.encode(key_bytes)),
        );
    }
    if let Some(pass) = gen_rcon_pass {
        table.insert(
            "factorio_rcon_pass".into(),
            toml::Value::String(pass.to_string()),
        );
    }
    if rcon_port_was_zero {
        table.insert(
            "factorio_rcon_port".into(),
            toml::Value::Integer(cfg.factorio_rcon_port as i64),
        );
    }
    if factorio_dir_changed {
        table.insert(
            "factorio_dir".into(),
            toml::Value::String(cfg.factorio_dir.clone()),
        );
    }
    if let Err(e) = std::fs::write(&path, toml::to_string_pretty(&table).unwrap_or_default()) {
        warn!(error=%e, path=%path, "failed to persist generated config");
    } else {
        info!(path=%path, "persisted generated config values");
    }
}
