use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use tower_http::limit::RequestBodyLimitLayer;

use crate::{
    error::{AppError, AppResult},
    middleware,
    services::{mod_pack, mod_portal, mods},
    state::AppState,
};

pub fn router(state: AppState) -> Router<AppState> {
    let pack_upload_limit: usize = 200 * 1024 * 1024; // 200MB
    Router::new()
        .route("/mods/packs/list", get(list_packs))
        .route("/mods/packs/create", post(create_pack))
        .route("/mods/packs/{pack}/delete", post(delete_pack))
        .route("/mods/packs/{pack}/download", get(download_pack))
        .route(
            "/mods/packs/{pack}/load",
            post(load_pack).route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                middleware::server_off,
            )),
        )
        .route("/mods/packs/{pack}/list", get(list_pack_mods))
        .route("/mods/packs/{pack}/mod/toggle", post(toggle_pack_mod))
        .route("/mods/packs/{pack}/mod/delete", post(delete_pack_mod))
        .route("/mods/packs/{pack}/mod/update", post(update_pack_mod))
        .route(
            "/mods/packs/{pack}/mod/upload",
            post(upload_pack_mod).route_layer(RequestBodyLimitLayer::new(pack_upload_limit)),
        )
        .route(
            "/mods/packs/{pack}/portal/install",
            post(pack_portal_install),
        )
        .route(
            "/mods/packs/{pack}/portal/install/multiple",
            post(pack_portal_install_multiple),
        )
        .with_state(state)
}

async fn list_packs(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<mod_pack::ModPackResult>>> {
    let list = mod_pack::list(&state).await?;
    Ok(Json(list))
}

#[derive(serde::Deserialize)]
struct CreatePackReq {
    name: String,
}

async fn create_pack(
    State(state): State<AppState>,
    Json(req): Json<CreatePackReq>,
) -> AppResult<Json<Vec<mod_pack::ModPackResult>>> {
    let list = mod_pack::create_from_current(&state, &req.name).await?;
    Ok(Json(list))
}

async fn delete_pack(
    State(state): State<AppState>,
    Path(pack): Path<String>,
) -> AppResult<Json<String>> {
    let name = mod_pack::delete(&state, &pack).await?;
    Ok(Json(name))
}

async fn download_pack(
    State(state): State<AppState>,
    Path(pack): Path<String>,
) -> AppResult<impl axum::response::IntoResponse> {
    use crate::error::AppError;
    use axum::http::{header, HeaderValue};
    use tokio_util::io::ReaderStream;
    let dir = std::path::Path::new(&state.config.mod_pack_dir)
        .join(&pack)
        .to_string_lossy()
        .to_string();
    let tmp = std::env::temp_dir().join(format!("pack_{}.zip", uuid::Uuid::new_v4()));
    // Build zip into temp file (blocking); helper includes cross-process shared lock
    let dir2 = dir.clone();
    let out = tmp.clone();
    tokio::task::spawn_blocking(move || mods::build_zip_of_dir_to_path(&dir2, &out))
        .await
        .map_err(|e| AppError::Config {
            msg: format!("zip task join error: {}", e),
        })??;
    let file = tokio::fs::File::open(&tmp)
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(&tmp);
    }
    let stream = ReaderStream::new(file);
    let mut resp = axum::http::Response::new(axum::body::Body::from_stream(stream));
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip;charset=UTF-8"),
    );
    let cd = format!("attachment; filename=\"{}.zip\"", pack);
    resp.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&cd).unwrap_or(HeaderValue::from_static("attachment")),
    );
    Ok(resp)
}

async fn load_pack(
    State(state): State<AppState>,
    Path(pack): Path<String>,
) -> AppResult<Json<mod_pack::ModsObject>> {
    let mods = mod_pack::load_into_current(&state, &pack).await?;
    Ok(Json(mods))
}

async fn list_pack_mods(
    State(state): State<AppState>,
    Path(pack): Path<String>,
) -> AppResult<Json<mod_pack::ModsObject>> {
    let mods = mod_pack::list_mods_in_pack(&state, &pack).await?;
    Ok(Json(mods))
}

#[derive(serde::Deserialize)]
struct ToggleReq {
    name: String,
}

async fn toggle_pack_mod(
    State(state): State<AppState>,
    Path(pack): Path<String>,
    Json(req): Json<ToggleReq>,
) -> AppResult<Json<bool>> {
    let val = mod_pack::toggle_mod(&state, &pack, &req.name).await?;
    Ok(Json(val))
}

#[derive(serde::Deserialize)]
struct DeleteReq {
    name: String,
}

async fn delete_pack_mod(
    State(state): State<AppState>,
    Path(pack): Path<String>,
    Json(req): Json<DeleteReq>,
) -> AppResult<Json<bool>> {
    let val = mod_pack::delete_mod(&state, &pack, &req.name).await?;
    Ok(Json(val))
}

#[derive(serde::Deserialize)]
struct UpdateReq {
    #[serde(rename = "modName")]
    mod_name: String,
    #[serde(rename = "downloadUrl")]
    download_url: String,
    #[serde(rename = "fileName")]
    file_name: String,
}

async fn update_pack_mod(
    State(state): State<AppState>,
    Path(pack): Path<String>,
    Json(req): Json<UpdateReq>,
) -> AppResult<Json<mods::InstalledMod>> {
    let m = mod_pack::update_mod(
        &state,
        &pack,
        &req.mod_name,
        &req.download_url,
        &req.file_name,
    )
    .await?;
    Ok(Json(m))
}

async fn upload_pack_mod(
    State(state): State<AppState>,
    Path(pack): Path<String>,
    mut multipart: axum::extract::Multipart,
) -> AppResult<Json<mod_pack::ModsObject>> {
    use tokio::io::AsyncWriteExt;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest { msg: e.to_string() })?
    {
        if field.name() != Some("mod_file") {
            continue;
        }
        let mut filename = field.file_name().unwrap_or("upload.zip").to_string();
        filename = std::path::Path::new(&filename)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("upload.zip")
            .to_string();
        let is_zip = std::path::Path::new(&filename)
            .extension()
            .and_then(|s| s.to_str())
            == Some("zip");
        if !is_zip {
            return Err(AppError::BadRequest {
                msg: "uploaded file must be a .zip".into(),
            });
        }
        let dir = std::path::Path::new(&state.config.mod_pack_dir).join(&pack);
        tokio::fs::create_dir_all(&dir).await.ok();
        let dest = dir.join(&filename);
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
        let obj = mod_pack::list_mods_in_pack(&state, &pack).await?;
        return Ok(Json(obj));
    }
    Err(AppError::BadRequest {
        msg: "no mod_file provided".into(),
    })
}

#[derive(serde::Deserialize)]
struct PortalInstallReq {
    #[serde(rename = "downloadUrl")]
    download_url: String,
    #[serde(rename = "fileName")]
    file_name: String,
    #[serde(rename = "modName")]
    mod_name: String,
}

async fn pack_portal_install(
    State(state): State<AppState>,
    Path(pack): Path<String>,
    Json(req): Json<PortalInstallReq>,
) -> AppResult<Json<serde_json::Value>> {
    let dir = std::path::Path::new(&state.config.mod_pack_dir)
        .join(&pack)
        .to_string_lossy()
        .to_string();
    let mods = mod_portal::install_one_into_dir(
        &state,
        &dir,
        &req.download_url,
        &req.file_name,
        &req.mod_name,
    )
    .await?;
    Ok(Json(serde_json::json!({ "mods": mods })))
}

async fn pack_portal_install_multiple(
    State(state): State<AppState>,
    Path(pack): Path<String>,
    Json(payload): Json<Vec<serde_json::Value>>,
) -> AppResult<Json<serde_json::Value>> {
    let mut items: Vec<(String, String)> = Vec::new();
    for v in payload.into_iter() {
        let name = v
            .get("name")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string();
        let version = v
            .get("version")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string();
        if !name.is_empty() && !version.is_empty() {
            items.push((name, version));
        }
    }
    let dir = std::path::Path::new(&state.config.mod_pack_dir)
        .join(&pack)
        .to_string_lossy()
        .to_string();
    let mods = mod_portal::install_multiple_into_dir(&state, &dir, &items).await?;
    Ok(Json(serde_json::json!({ "mods": mods })))
}
