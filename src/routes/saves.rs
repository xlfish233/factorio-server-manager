use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::{
    cmp::Reverse,
    fs,
    path::{Path as FsPath, PathBuf},
};
use tokio_util::io::ReaderStream;
use tower_http::limit::RequestBodyLimitLayer;

use crate::{
    error::{AppError, AppResult},
    factorio, middleware, save_parser,
    state::AppState,
};

#[derive(Deserialize)]
struct ListParams {
    #[serde(default)]
    latest: bool,
}

#[derive(serde::Serialize)]
struct SaveEntry {
    name: String,
    last_mod: String,
    size: u64,
}

pub fn router(state: AppState) -> Router<AppState> {
    let save_upload_limit: usize = 512 * 1024 * 1024; // 512MB for save uploads
    Router::new()
        .route("/saves/list", get(list_saves))
        .route("/saves/dl/{save}", get(dl_save))
        .route(
            "/saves/upload",
            post(upload_save).route_layer(RequestBodyLimitLayer::new(save_upload_limit)),
        )
        .route("/saves/rm/{save}", get(remove_save))
        .route(
            "/saves/mods",
            post(load_mods_from_save).route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                middleware::server_off,
            )),
        )
        .route(
            "/saves/create/{save}",
            get(create_save).route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                middleware::server_off,
            )),
        )
}

async fn list_saves(
    State(state): State<AppState>,
    Query(ListParams { latest }): Query<ListParams>,
) -> AppResult<Json<Vec<SaveEntry>>> {
    let dir = &state.config.saves_dir;
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| AppError::Config {
            msg: format!("Error listing save files: {}", e),
        })?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path();
            // only .zip files and ignore Factorio temporary files like *.tmp.zip
            let is_zip = p.extension().map(|ext| ext == "zip").unwrap_or(false);
            if !is_zip {
                return false;
            }
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            !name.ends_with(".tmp.zip")
        })
        .filter_map(|e| {
            let md = e.metadata().ok()?;
            let modified = md
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs();
            Some((
                e.file_name().to_string_lossy().to_string(),
                md.len(),
                modified,
            ))
        })
        .collect();
    // Sort by modified desc
    entries.sort_by_key(|(_, _, m)| Reverse(*m));
    let mut resp: Vec<SaveEntry> = entries
        .iter()
        .map(|(n, size, m)| SaveEntry {
            name: n.clone(),
            last_mod: chrono::DateTime::<chrono::Utc>::from(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(*m),
            )
            .to_rfc3339(),
            size: *size,
        })
        .collect();
    if latest && !resp.is_empty() {
        let name = resp[0].name.clone();
        resp.push(SaveEntry {
            name: format!("Load Latest ({})", name),
            last_mod: resp[0].last_mod.clone(),
            size: resp[0].size,
        });
    }
    Ok(Json(resp))
}

async fn dl_save(
    State(state): State<AppState>,
    Path(save): Path<String>,
) -> AppResult<impl IntoResponse> {
    let path = FsPath::new(&state.config.saves_dir).join(&save);
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| AppError::BadRequest {
            msg: format!("failed to open save: {}", e),
        })?;
    let stream = ReaderStream::new(file);
    let body = axum::body::Body::from_stream(stream);
    let mut resp = axum::http::Response::new(body);
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    let cd = format!("attachment; filename=\"{}\"", save);
    resp.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&cd).unwrap_or(HeaderValue::from_static("attachment")),
    );
    Ok(resp)
}

async fn upload_save(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> AppResult<String> {
    use tokio::io::AsyncWriteExt;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest { msg: e.to_string() })?
    {
        if field.name() != Some("savefile") {
            continue;
        }
        let mut file_name = field.file_name().unwrap_or("upload.zip").to_string();
        // sanitize filename (basename only)
        file_name = std::path::Path::new(&file_name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("upload.zip")
            .to_string();
        let ext = std::path::Path::new(&file_name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if ext != "zip" {
            return Err(AppError::BadRequest {
                msg: format!("Fileformat {{{}}} is not allowed", ext),
            });
        }
        let dest = PathBuf::from(&state.config.saves_dir).join(&file_name);
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
        let mut f = tokio::fs::File::create(&dest)
            .await
            .map_err(|e| AppError::Config { msg: e.to_string() })?;
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| AppError::Config { msg: e.to_string() })?
        {
            f.write_all(&chunk)
                .await
                .map_err(|e| AppError::Config { msg: e.to_string() })?;
        }
    }
    Ok("Uploading files successful".into())
}

async fn remove_save(State(state): State<AppState>, Path(save): Path<String>) -> AppResult<String> {
    let path = PathBuf::from(&state.config.saves_dir).join(&save);
    if !path.exists() {
        return Err(AppError::BadRequest {
            msg: format!("save not found: {}", save),
        });
    }
    tokio::fs::remove_file(&path)
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    Ok(format!("Removed save: {}", save))
}

async fn create_save(State(state): State<AppState>, Path(save): Path<String>) -> AppResult<String> {
    if save.trim().is_empty() {
        return Err(AppError::BadRequest {
            msg: "no save name provided".into(),
        });
    }
    let path = PathBuf::from(&state.config.saves_dir).join(&save);
    let out = factorio::create_save(&state.config, &path.to_string_lossy()).await?;
    Ok(format!(
        "Save {} created successfully. Command output: \n{}",
        save, out
    ))
}

#[derive(serde::Deserialize)]
struct SaveModsReq {
    #[serde(rename = "saveFile")]
    save_file: String,
}

#[derive(serde::Serialize)]
struct SaveModsResp {
    mods: Vec<SaveModEntry>,
}

#[derive(serde::Serialize)]
struct SaveModEntry {
    name: String,
    version: String,
}

async fn load_mods_from_save(
    State(state): State<AppState>,
    Json(req): Json<SaveModsReq>,
) -> AppResult<Json<SaveModsResp>> {
    // Validate file exists (under saves_dir)
    let path = PathBuf::from(&state.config.saves_dir).join(&req.save_file);
    if !path.exists() {
        return Err(AppError::BadRequest {
            msg: format!("save not found: {}", req.save_file),
        });
    }
    let mods = save_parser::extract_mods_from_save_zip(&path.to_string_lossy())?;
    let mods = mods
        .into_iter()
        .map(|m| SaveModEntry {
            name: m.name,
            version: m.version,
        })
        .collect();
    Ok(Json(SaveModsResp { mods }))
}
