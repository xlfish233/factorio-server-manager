use crate::error::{AppError, AppResult};
use crate::services::mods;
use crate::state::AppState;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
pub struct ModsObject {
    pub mods: Vec<mods::InstalledMod>,
}

#[derive(Serialize)]
pub struct ModPackResult {
    pub name: String,
    pub mods: ModsObject,
}

fn pack_dir(state: &AppState) -> &str {
    &state.config.mod_pack_dir
}

pub async fn list(state: &AppState) -> AppResult<Vec<ModPackResult>> {
    let root = PathBuf::from(pack_dir(state));
    let mut out = Vec::new();
    let mut dir = match tokio::fs::read_dir(&root).await {
        Ok(rd) => rd,
        Err(_) => return Ok(out),
    };
    while let Some(entry) = dir
        .next_entry()
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?
    {
        let ft = entry
            .file_type()
            .await
            .map_err(|e| AppError::Config { msg: e.to_string() })?;
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let abs = root.join(&name);
        let mods_list = mods::list_installed_in_dir(&abs.to_string_lossy())?;
        out.push(ModPackResult {
            name,
            mods: ModsObject { mods: mods_list },
        });
    }
    Ok(out)
}

pub async fn create_from_current(state: &AppState, name: &str) -> AppResult<Vec<ModPackResult>> {
    let pack_root = PathBuf::from(pack_dir(state)).join(name);
    if tokio::fs::try_exists(&pack_root).await.unwrap_or(false) {
        return Err(AppError::BadRequest {
            msg: format!("ModPack {} already exists", name),
        });
    }
    let _read_guard = state.mods_lock.read().await; // snapshot while copying
    tokio::fs::create_dir_all(&pack_root)
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    let mut rd = tokio::fs::read_dir(&state.config.mods_dir)
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    while let Some(entry) = rd
        .next_entry()
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?
    {
        let md = entry
            .metadata()
            .await
            .map_err(|e| AppError::Config { msg: e.to_string() })?;
        if !md.is_dir() {
            let src = entry.path();
            let dst = pack_root.join(entry.file_name());
            let bytes = tokio::fs::read(&src)
                .await
                .map_err(|e| AppError::Config { msg: e.to_string() })?;
            tokio::fs::write(&dst, &bytes)
                .await
                .map_err(|e| AppError::Config { msg: e.to_string() })?;
        }
    }
    list(state).await
}

pub async fn delete(state: &AppState, name: &str) -> AppResult<String> {
    let path = PathBuf::from(pack_dir(state)).join(name);
    tokio::fs::remove_dir_all(&path)
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    Ok(name.to_string())
}

// download_zip is unused by routes; pack download is implemented streaming via a temp file

pub async fn load_into_current(state: &AppState, name: &str) -> AppResult<ModsObject> {
    let src_dir = PathBuf::from(pack_dir(state)).join(name);
    if !tokio::fs::try_exists(&src_dir).await.unwrap_or(false) {
        return Err(AppError::NotFound {
            msg: "modpack not found".into(),
        });
    }
    // Write lock on current mods dir
    let _guard = state.mods_lock.write().await;
    // Remove mods dir and recreate
    let dst_dir = PathBuf::from(&state.config.mods_dir);
    let _ = tokio::fs::remove_dir_all(&dst_dir).await;
    tokio::fs::create_dir_all(&dst_dir)
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    let mut rd = tokio::fs::read_dir(&src_dir)
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    while let Some(entry) = rd
        .next_entry()
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?
    {
        let md = entry
            .metadata()
            .await
            .map_err(|e| AppError::Config { msg: e.to_string() })?;
        if !md.is_dir() {
            let src = entry.path();
            let dst = dst_dir.join(entry.file_name());
            let bytes = tokio::fs::read(&src)
                .await
                .map_err(|e| AppError::Config { msg: e.to_string() })?;
            tokio::fs::write(&dst, &bytes)
                .await
                .map_err(|e| AppError::Config { msg: e.to_string() })?;
        }
    }
    let mods = mods::list_installed(state).await?;
    Ok(ModsObject { mods })
}

pub async fn list_mods_in_pack(state: &AppState, name: &str) -> AppResult<ModsObject> {
    let dir = PathBuf::from(pack_dir(state)).join(name);
    let mods = mods::list_installed_in_dir(&dir.to_string_lossy())?;
    Ok(ModsObject { mods })
}

pub async fn toggle_mod(state: &AppState, name: &str, mod_name: &str) -> AppResult<bool> {
    let dir = PathBuf::from(pack_dir(state)).join(name);
    mods::toggle_in_dir(&dir.to_string_lossy(), mod_name)
}

pub async fn delete_mod(state: &AppState, name: &str, mod_name: &str) -> AppResult<bool> {
    let dir = PathBuf::from(pack_dir(state)).join(name);
    let _ = mods::delete_in_dir(&dir.to_string_lossy(), mod_name)?;
    Ok(true)
}

pub async fn update_mod(
    state: &AppState,
    name: &str,
    mod_name: &str,
    download_url: &str,
    file_name: &str,
) -> AppResult<mods::InstalledMod> {
    let dir = PathBuf::from(pack_dir(state)).join(name);
    mods::update_from_url_in_dir(&dir.to_string_lossy(), mod_name, download_url, file_name).await
}

// upload_mod is unused by routes (they stream to disk directly)
