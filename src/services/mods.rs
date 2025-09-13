use serde::{Deserialize, Serialize};
use std::io::Read;
use std::{collections::HashMap, fs, io::Write};

use crate::{
    error::{AppError, AppResult},
    state::AppState,
};
use fs2::FileExt;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstalledMod {
    pub name: String,
    pub version: String,
    pub title: String,
    pub author: String,
    pub file_name: String,
    pub factorio_version: String,
    pub dependencies: Option<serde_json::Value>,
    pub compatibility: bool,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize)]
struct ModListJsonEntry {
    name: String,
    enabled: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct ModListJson {
    mods: Vec<ModListJsonEntry>,
}

fn read_mod_list_json(path: &str) -> ModListJson {
    let full = std::path::Path::new(path).join("mod-list.json");
    match fs::read_to_string(&full) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => ModListJson::default(),
    }
}

fn write_mod_list_json(path: &str, list: &ModListJson) -> AppResult<()> {
    let full = std::path::Path::new(path).join("mod-list.json");
    let json =
        serde_json::to_string_pretty(list).map_err(|e| AppError::Config { msg: e.to_string() })?;
    fs::write(&full, json).map_err(|e| AppError::Config { msg: e.to_string() })?;
    Ok(())
}

fn lock_path(dir: &str) -> std::path::PathBuf {
    std::path::Path::new(dir).join(".fsm_mods.lock")
}

fn lock_shared(dir: &str) -> AppResult<std::fs::File> {
    let p = lock_path(dir);
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(p)
        .map_err(|e| AppError::Config {
            msg: format!("lock open: {}", e),
        })?;
    f.lock_shared().map_err(|e| AppError::Config {
        msg: format!("lock shared: {}", e),
    })?;
    Ok(f)
}

fn lock_exclusive(dir: &str) -> AppResult<std::fs::File> {
    let p = lock_path(dir);
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(p)
        .map_err(|e| AppError::Config {
            msg: format!("lock open: {}", e),
        })?;
    f.lock_exclusive().map_err(|e| AppError::Config {
        msg: format!("lock exclusive: {}", e),
    })?;
    Ok(f)
}

// -------- Generic helpers operating on arbitrary mods directory (for mod packs) --------
pub(crate) fn list_installed_in_dir(dir: &str) -> AppResult<Vec<InstalledMod>> {
    let list = read_mod_list_json(dir);
    let enabled_map: HashMap<String, bool> =
        list.mods.into_iter().map(|e| (e.name, e.enabled)).collect();
    let mut out: Vec<InstalledMod> = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(out),
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|s| s.to_str()) != Some("zip") {
            continue;
        }
        let file_name = e.file_name().to_string_lossy().to_string();
        let mut mod_entry: Option<InstalledMod> = None;
        if let Ok(file) = fs::File::open(&path) {
            if let Ok(mut zip) = zip::ZipArchive::new(file) {
                let mut info_bytes = Vec::new();
                let mut found = false;
                if let Ok(mut f) = zip.by_name("info.json") {
                    use std::io::Read;
                    let _ = f.read_to_end(&mut info_bytes);
                    found = true;
                }
                if !found {
                    for i in 0..zip.len() {
                        if let Ok(mut f) = zip.by_index(i) {
                            let name = f.name().to_string();
                            if name.ends_with("/info.json") || name == "info.json" {
                                use std::io::Read;
                                let _ = f.read_to_end(&mut info_bytes);
                                found = true;
                                break;
                            }
                        }
                    }
                }
                if found {
                    #[derive(Deserialize)]
                    struct ModInfo {
                        name: String,
                        version: String,
                        #[serde(default)]
                        title: String,
                        #[serde(default)]
                        author: String,
                        #[serde(default)]
                        factorio_version: String,
                        #[serde(default)]
                        dependencies: Option<serde_json::Value>,
                    }
                    if let Ok(mi) = serde_json::from_slice::<ModInfo>(&info_bytes) {
                        let enabled = *enabled_map.get(&mi.name).unwrap_or(&true);
                        mod_entry = Some(InstalledMod {
                            name: mi.name.clone(),
                            version: mi.version.clone(),
                            title: if mi.title.is_empty() {
                                mi.name.clone()
                            } else {
                                mi.title.clone()
                            },
                            author: mi.author.clone(),
                            file_name: file_name.clone(),
                            factorio_version: if mi.factorio_version.is_empty() {
                                String::from("unknown")
                            } else {
                                mi.factorio_version.clone()
                            },
                            dependencies: mi.dependencies,
                            compatibility: true,
                            enabled,
                        });
                    }
                }
            }
        }
        if mod_entry.is_none() {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&file_name);
            let (name, version) = match stem.rsplit_once('_') {
                Some((n, v)) => (n.to_string(), v.to_string()),
                None => (stem.to_string(), String::new()),
            };
            let enabled = *enabled_map.get(&name).unwrap_or(&true);
            mod_entry = Some(InstalledMod {
                name: name.clone(),
                version: version.clone(),
                title: name.clone(),
                author: String::new(),
                file_name: file_name.clone(),
                factorio_version: String::from("unknown"),
                dependencies: None,
                compatibility: true,
                enabled,
            });
        }
        out.push(mod_entry.unwrap());
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub(crate) fn toggle_in_dir(dir: &str, name: &str) -> AppResult<bool> {
    let mut list = read_mod_list_json(dir);
    if let Some(entry) = list.mods.iter_mut().find(|e| e.name == name) {
        entry.enabled = !entry.enabled;
        let val = entry.enabled;
        write_mod_list_json(dir, &list)?;
        Ok(val)
    } else {
        list.mods.push(ModListJsonEntry {
            name: name.to_string(),
            enabled: true,
        });
        write_mod_list_json(dir, &list)?;
        Ok(true)
    }
}

pub(crate) fn delete_in_dir(dir: &str, name: &str) -> AppResult<String> {
    let mut found = false;
    for e in fs::read_dir(dir)
        .map_err(|e| AppError::Config { msg: e.to_string() })?
        .flatten()
    {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) == Some("zip") {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                if let Some((mod_name, _ver)) = stem.rsplit_once('_') {
                    if mod_name == name {
                        fs::remove_file(&p).map_err(|e| AppError::Config { msg: e.to_string() })?;
                        found = true;
                    }
                }
            }
        }
    }
    let _ = found; // lenient
    Ok(name.to_string())
}

// delete_all_in_dir was unused and has been removed

pub(crate) fn upload_file_in_dir(
    dir: &str,
    filename: &str,
    bytes: &[u8],
    content_is_zip: bool,
) -> AppResult<()> {
    let dest = std::path::Path::new(dir).join(filename);
    if content_is_zip {
        if std::path::Path::new(filename)
            .extension()
            .and_then(|s| s.to_str())
            != Some("zip")
        {
            return Err(AppError::BadRequest {
                msg: "Uploaded file is not a zip".into(),
            });
        }
    } else if filename != "mod-settings.dat" && filename != "mod-list.json" {
        return Err(AppError::BadRequest {
            msg: "The uploaded file wasn't a zip-file, a mod-settings.dat or a mod-list.json"
                .into(),
        });
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).ok();
    }
    let mut f = fs::File::create(&dest).map_err(|e| AppError::Config { msg: e.to_string() })?;
    use std::io::Write;
    f.write_all(bytes)
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    Ok(())
}

// build_zip_of_dir function was unused; retained functionality via build_zip_of_dir_to_path

pub(crate) async fn update_from_url_in_dir(
    dir: &str,
    mod_name: &str,
    download_url: &str,
    file_name: &str,
) -> AppResult<InstalledMod> {
    let bytes = reqwest::get(download_url)
        .await
        .map_err(|e| AppError::Config {
            msg: format!("download error: {}", e),
        })?
        .bytes()
        .await
        .map_err(|e| AppError::Config {
            msg: format!("download read error: {}", e),
        })?;
    upload_file_in_dir(dir, file_name, &bytes, true)?;
    let mods = list_installed_in_dir(dir)?;
    if let Some(m) = mods.into_iter().find(|m| m.name == mod_name) {
        Ok(m)
    } else {
        Err(AppError::NotFound {
            msg: format!("Could not find mod {} after update", mod_name),
        })
    }
}

pub async fn list_installed(state: &AppState) -> AppResult<Vec<InstalledMod>> {
    let _guard = state.mods_lock.read().await;
    let dir = &state.config.mods_dir;
    // Build enabled map from mod-list.json
    let list = read_mod_list_json(dir);
    let enabled_map: HashMap<String, bool> =
        list.mods.into_iter().map(|e| (e.name, e.enabled)).collect();

    let mut out: Vec<InstalledMod> = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(out),
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|s| s.to_str()) != Some("zip") {
            continue;
        }
        let file_name = e.file_name().to_string_lossy().to_string();
        // Try to parse info.json for richer metadata
        let mut mod_entry: Option<InstalledMod> = None;
        if let Ok(file) = fs::File::open(&path) {
            if let Ok(mut zip) = zip::ZipArchive::new(file) {
                // find info.json (may exist at root or in subfolder)
                let mut info_bytes = Vec::new();
                // Attempt common paths first
                let candidates = [
                    "info.json",
                    // Some mods wrap content in a folder; search all files
                ];
                let mut found = false;
                for cand in candidates {
                    if let Ok(mut f) = zip.by_name(cand) {
                        let _ = f.read_to_end(&mut info_bytes);
                        found = true;
                        break;
                    }
                }
                if !found {
                    // Brute-force search for any filename ending with info.json
                    for i in 0..zip.len() {
                        if let Ok(mut f) = zip.by_index(i) {
                            let name = f.name().to_string();
                            if name.ends_with("/info.json") || name == "info.json" {
                                let _ = f.read_to_end(&mut info_bytes);
                                found = true;
                                break;
                            }
                        }
                    }
                }
                if found {
                    #[derive(Deserialize)]
                    struct ModInfo {
                        name: String,
                        version: String,
                        #[serde(default)]
                        title: String,
                        #[serde(default)]
                        author: String,
                        #[serde(default)]
                        factorio_version: String,
                        #[serde(default)]
                        dependencies: Option<serde_json::Value>,
                    }
                    if let Ok(mi) = serde_json::from_slice::<ModInfo>(&info_bytes) {
                        let enabled = *enabled_map.get(&mi.name).unwrap_or(&true);
                        mod_entry = Some(InstalledMod {
                            name: mi.name.clone(),
                            version: mi.version.clone(),
                            title: if mi.title.is_empty() {
                                mi.name.clone()
                            } else {
                                mi.title.clone()
                            },
                            author: mi.author.clone(),
                            file_name: file_name.clone(),
                            factorio_version: if mi.factorio_version.is_empty() {
                                "unknown".into()
                            } else {
                                mi.factorio_version.clone()
                            },
                            dependencies: mi.dependencies,
                            compatibility: true,
                            enabled,
                        });
                    }
                }
            }
        }
        if mod_entry.is_none() {
            // Fallback: derive from filename
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&file_name);
            let (name, version) = match stem.rsplit_once('_') {
                Some((n, v)) => (n.to_string(), v.to_string()),
                None => (stem.to_string(), String::new()),
            };
            let enabled = *enabled_map.get(&name).unwrap_or(&true);
            mod_entry = Some(InstalledMod {
                name: name.clone(),
                version: version.clone(),
                title: name.clone(),
                author: String::new(),
                file_name: file_name.clone(),
                factorio_version: String::from("unknown"),
                dependencies: None,
                compatibility: true,
                enabled,
            });
        }
        out.push(mod_entry.unwrap());
    }
    // Sort by name for stable output
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub async fn toggle(state: &AppState, name: &str) -> AppResult<bool> {
    let _guard = state.mods_lock.write().await;
    let _lock = lock_exclusive(&state.config.mods_dir)?;
    let dir = &state.config.mods_dir;
    let mut list = read_mod_list_json(dir);
    // ensure base present
    if !list.mods.iter().any(|e| e.name == "base") {
        list.mods.push(ModListJsonEntry {
            name: "base".into(),
            enabled: true,
        });
    }
    if let Some(entry) = list.mods.iter_mut().find(|e| e.name == name) {
        entry.enabled = !entry.enabled;
        let val = entry.enabled;
        write_mod_list_json(dir, &list)?;
        Ok(val)
    } else {
        // if missing, add and enable
        list.mods.push(ModListJsonEntry {
            name: name.to_string(),
            enabled: true,
        });
        write_mod_list_json(dir, &list)?;
        Ok(true)
    }
}

pub async fn delete(state: &AppState, name: &str) -> AppResult<String> {
    let _guard = state.mods_lock.write().await;
    let _lock = lock_exclusive(&state.config.mods_dir)?;
    let dir = &state.config.mods_dir;
    let mut found = false;
    for e in fs::read_dir(dir)
        .map_err(|e| AppError::Config { msg: e.to_string() })?
        .flatten()
    {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) == Some("zip") {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                if let Some((mod_name, _ver)) = stem.rsplit_once('_') {
                    if mod_name == name {
                        // delete this zip
                        fs::remove_file(&p).map_err(|e| AppError::Config { msg: e.to_string() })?;
                        found = true;
                    }
                }
            }
        }
    }
    if !found {
        // it's acceptable to return name even if nothing deleted, to mirror lenient behavior
    }
    Ok(name.to_string())
}

pub async fn delete_all(state: &AppState) -> AppResult<()> {
    let _guard = state.mods_lock.write().await;
    let _lock = lock_exclusive(&state.config.mods_dir)?;
    let dir = &state.config.mods_dir;
    // Remove the whole mods dir (like Go impl) and recreate
    if fs::remove_dir_all(dir).is_err() {
        // ignore error if it doesn't exist
    }
    fs::create_dir_all(dir).map_err(|e| AppError::Config { msg: e.to_string() })?;
    Ok(())
}

pub async fn upload_file(
    state: &AppState,
    filename: &str,
    bytes: &[u8],
    content_is_zip: bool,
) -> AppResult<()> {
    let _guard = state.mods_lock.write().await;
    let _lock = lock_exclusive(&state.config.mods_dir)?;
    let dir = &state.config.mods_dir;
    let dest = std::path::Path::new(dir).join(filename);
    // accept .zip or special names mod-settings.dat/mod-list.json
    if content_is_zip {
        if std::path::Path::new(filename)
            .extension()
            .and_then(|s| s.to_str())
            != Some("zip")
        {
            return Err(AppError::BadRequest {
                msg: "Uploaded file is not a zip".into(),
            });
        }
    } else if filename != "mod-settings.dat" && filename != "mod-list.json" {
        return Err(AppError::BadRequest {
            msg: "The uploaded file wasn't a zip-file, a mod-settings.dat or a mod-list.json"
                .into(),
        });
    }

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).ok();
    }
    let mut f = fs::File::create(&dest).map_err(|e| AppError::Config { msg: e.to_string() })?;
    f.write_all(bytes)
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    Ok(())
}

// build_zip_of_all was unused; pack routes use build_zip_of_dir_to_path instead

pub async fn update_from_url(
    state: &AppState,
    mod_name: &str,
    download_url: &str,
    file_name: &str,
) -> AppResult<InstalledMod> {
    let _guard = state.mods_lock.write().await;
    let _lock = lock_exclusive(&state.config.mods_dir)?;
    update_from_url_in_dir(&state.config.mods_dir, mod_name, download_url, file_name).await
}

pub(crate) fn build_zip_of_dir_to_path(dir: &str, out_path: &std::path::Path) -> AppResult<()> {
    use zip::write::FileOptions;
    // cross-process shared lock during read
    let _lock = lock_shared(dir)?;
    let mut file =
        std::fs::File::create(out_path).map_err(|e| AppError::Config { msg: e.to_string() })?;
    let mut writer = zip::ZipWriter::new(std::io::BufWriter::new(&mut file));
    let options = FileOptions::default();
    for entry in std::fs::read_dir(dir).map_err(|e| AppError::Config { msg: e.to_string() })? {
        let e = entry.map_err(|e| AppError::Config { msg: e.to_string() })?;
        let p = e.path();
        if p.is_file() {
            let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let data = std::fs::read(&p).map_err(|e| AppError::Config { msg: e.to_string() })?;
            writer
                .start_file(name, options)
                .map_err(|e| AppError::Config { msg: e.to_string() })?;
            use std::io::Write;
            writer
                .write_all(&data)
                .map_err(|e| AppError::Config { msg: e.to_string() })?;
        }
    }
    writer
        .finish()
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    Ok(())
}
