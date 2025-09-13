use serde::{Deserialize, Serialize};

use crate::{
    config::Config,
    error::{AppError, AppResult},
    services::mods,
    state::AppState,
};

fn creds_path(cfg: &Config) -> &str {
    &cfg.credentials_file
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Credentials {
    username: String,
    userkey: String,
}

pub async fn login(state: &AppState, username: &str, password: &str) -> AppResult<()> {
    // POST form to Factorio Auth API
    let params = [
        ("require_game_ownership", "true"),
        ("username", username),
        ("password", password),
    ];
    let resp = reqwest::Client::new()
        .post("https://auth.factorio.com/api-login")
        .form(&params)
        .send()
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    let status = resp.status();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    if !status.is_success() {
        return Err(AppError::BadRequest {
            msg: String::from_utf8_lossy(&bytes).to_string(),
        });
    }
    // Response is JSON array like ["TOKEN"]
    let keys: Vec<String> =
        serde_json::from_slice(&bytes).map_err(|e| AppError::Config { msg: e.to_string() })?;
    let token = keys.first().cloned().unwrap_or_default();
    if token.is_empty() {
        return Err(AppError::BadRequest {
            msg: "Empty token from Factorio auth".into(),
        });
    }
    let creds = Credentials {
        username: username.to_string(),
        userkey: token,
    };
    let data = serde_json::to_vec(&creds).map_err(|e| AppError::Config { msg: e.to_string() })?;
    std::fs::write(creds_path(&state.config), data)
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    Ok(())
}

pub async fn login_status(state: &AppState) -> AppResult<bool> {
    let path = creds_path(&state.config);
    let data = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return Ok(false),
    };
    let creds: Credentials = match serde_json::from_slice(&data) {
        Ok(c) => c,
        Err(_) => return Ok(false),
    };
    Ok(!creds.username.is_empty() && !creds.userkey.is_empty())
}

pub async fn logout(state: &AppState) -> AppResult<bool> {
    let path = creds_path(&state.config);
    match std::fs::remove_file(path) {
        Ok(_) => Ok(false),
        Err(_) => Ok(false),
    }
}

pub async fn list_mods_from_portal() -> AppResult<serde_json::Value> {
    let url = "https://mods.factorio.com/api/mods?page_size=max";
    let resp = reqwest::get(url)
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    if !status.is_success() {
        return Err(AppError::BadRequest { msg: text });
    }
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| AppError::Config { msg: e.to_string() })?;
    Ok(json)
}

#[derive(Deserialize, Serialize)]
pub struct ModDetailsRelease {
    #[serde(rename = "download_url")]
    pub download_url: String,
    #[serde(rename = "file_name")]
    pub file_name: String,
    pub version: String,
}

#[derive(Deserialize, Serialize)]
pub struct ModDetails {
    pub name: String,
    pub releases: Vec<ModDetailsRelease>,
}

pub async fn mod_info(mod_name: &str) -> AppResult<ModDetails> {
    let url = format!("https://mods.factorio.com/api/mods/{}", mod_name);
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    if !status.is_success() {
        return Err(AppError::BadRequest { msg: text });
    }
    let details: ModDetails =
        serde_json::from_str(&text).map_err(|e| AppError::Config { msg: e.to_string() })?;
    Ok(details)
}

fn build_portal_download_url(base: &str, creds: &Credentials) -> String {
    // base like "/download/modname/release_id"
    format!(
        "https://mods.factorio.com{}?username={}&token={}",
        base, creds.username, creds.userkey
    )
}

fn load_creds(cfg: &Config) -> AppResult<Credentials> {
    let data = std::fs::read(creds_path(cfg)).map_err(|e| AppError::BadRequest {
        msg: format!("credentials missing: {}", e),
    })?;
    let creds: Credentials = serde_json::from_slice(&data).map_err(|e| AppError::BadRequest {
        msg: format!("invalid credentials: {}", e),
    })?;
    if creds.username.is_empty() || creds.userkey.is_empty() {
        return Err(AppError::BadRequest {
            msg: "credentials invalid".into(),
        });
    }
    Ok(creds)
}

pub async fn install_one(
    state: &AppState,
    download_url: &str,
    file_name: &str,
    _mod_name: &str,
) -> AppResult<Vec<mods::InstalledMod>> {
    let creds = load_creds(&state.config)?;
    let full = build_portal_download_url(download_url, &creds);
    let bytes = reqwest::get(full)
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?
        .bytes()
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    mods::upload_file(state, file_name, &bytes, true).await?;
    let list = mods::list_installed(state).await?;
    Ok(list)
}

pub async fn install_one_into_dir(
    state: &AppState,
    dir: &str,
    download_url: &str,
    file_name: &str,
    _mod_name: &str,
) -> AppResult<Vec<mods::InstalledMod>> {
    let creds = load_creds(&state.config)?;
    let full = build_portal_download_url(download_url, &creds);
    let bytes = reqwest::get(full)
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?
        .bytes()
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    mods::upload_file_in_dir(dir, file_name, &bytes, true)?;
    let list = mods::list_installed_in_dir(dir)?;
    Ok(list)
}

pub async fn install_multiple(
    state: &AppState,
    mods_to_install: &[(String, String)],
) -> AppResult<Vec<mods::InstalledMod>> {
    for (name, version) in mods_to_install.iter() {
        if name == "base" {
            continue;
        }
        let details = mod_info(name).await?;
        let mut found = None;
        for r in details.releases.iter() {
            if &r.version == version {
                found = Some((
                    r.download_url.clone(),
                    r.file_name.clone(),
                    details.name.clone(),
                ));
                break;
            }
        }
        if let Some((dl, file, mod_name)) = found {
            let _ = install_one(state, &dl, &file, &mod_name).await?;
        } else {
            return Err(AppError::Config {
                msg: format!("version not found for {} {}", name, version),
            });
        }
    }
    let list = mods::list_installed(state).await?;
    Ok(list)
}

pub async fn install_multiple_into_dir(
    state: &AppState,
    dir: &str,
    mods_to_install: &[(String, String)],
) -> AppResult<Vec<mods::InstalledMod>> {
    for (name, version) in mods_to_install.iter() {
        if name == "base" {
            continue;
        }
        let details = mod_info(name).await?;
        let mut found = None;
        for r in details.releases.iter() {
            if &r.version == version {
                found = Some((
                    r.download_url.clone(),
                    r.file_name.clone(),
                    details.name.clone(),
                ));
                break;
            }
        }
        if let Some((dl, file, _mod_name)) = found {
            let _ = install_one_into_dir(state, dir, &dl, &file, name).await?;
        } else {
            return Err(AppError::Config {
                msg: format!("version not found for {} {}", name, version),
            });
        }
    }
    let list = mods::list_installed_in_dir(dir)?;
    Ok(list)
}
