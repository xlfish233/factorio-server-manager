use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};

use crate::{
    error::AppResult, middleware, services::mod_portal as portal_svc, services::mods as mods_svc,
    state::AppState,
};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use tower_http::limit::RequestBodyLimitLayer;

pub fn router(state: AppState) -> Router<AppState> {
    let mod_upload_limit: usize = 100 * 1024 * 1024; // 100MB
    Router::new()
        .route("/mods/list", get(list_installed))
        .route(
            "/mods/toggle",
            post(toggle).route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                middleware::server_off,
            )),
        )
        .route(
            "/mods/delete",
            post(delete).route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                middleware::server_off,
            )),
        )
        .route(
            "/mods/delete/all",
            post(delete_all).route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                middleware::server_off,
            )),
        )
        .route(
            "/mods/upload",
            post(upload)
                .route_layer(RequestBodyLimitLayer::new(mod_upload_limit))
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    middleware::server_off,
                )),
        )
        .route("/mods/download", get(download_all))
        .route(
            "/mods/update",
            post(update).route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                middleware::server_off,
            )),
        )
        // Mod Portal
        .route("/mods/portal/list", get(portal_list))
        .route("/mods/portal/info/{mod}", get(portal_info))
        .route("/mods/portal/loginstatus", get(portal_login_status))
        .route("/mods/portal/logout", get(portal_logout))
        .route("/mods/portal/login", post(portal_login))
        .route(
            "/mods/portal/install",
            post(portal_install).route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                middleware::server_off,
            )),
        )
        .route(
            "/mods/portal/install/multiple",
            post(portal_install_multiple).route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                middleware::server_off,
            )),
        )
        .with_state(state)
}

async fn list_installed(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<mods_svc::InstalledMod>>> {
    let mods = mods_svc::list_installed(&state).await?;
    Ok(Json(mods))
}

#[derive(serde::Deserialize)]
struct ToggleReq {
    name: String,
}

async fn toggle(
    State(state): State<AppState>,
    Json(req): Json<ToggleReq>,
) -> AppResult<Json<bool>> {
    let val = mods_svc::toggle(&state, &req.name).await?;
    Ok(Json(val))
}

#[derive(serde::Deserialize)]
struct DeleteReq {
    name: String,
}

async fn delete(
    State(state): State<AppState>,
    Json(req): Json<DeleteReq>,
) -> AppResult<Json<String>> {
    let name = mods_svc::delete(&state, &req.name).await?;
    Ok(Json(name))
}

async fn delete_all(State(state): State<AppState>) -> AppResult<Json<serde_json::Value>> {
    mods_svc::delete_all(&state).await?;
    Ok(Json(serde_json::Value::Null))
}

async fn upload(
    State(state): State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> AppResult<Json<serde_json::Value>> {
    use crate::error::AppError;
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
        // Validate accepted filenames
        if !is_zip && filename != "mod-settings.dat" && filename != "mod-list.json" {
            return Err(AppError::BadRequest {
                msg: "The uploaded file wasn't a zip-file, a mod-settings.dat or a mod-list.json"
                    .into(),
            });
        }
        let dest = std::path::Path::new(&state.config.mods_dir).join(&filename);
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
    let mods = mods_svc::list_installed(&state).await?;
    Ok(Json(serde_json::json!({ "mods": mods })))
}

async fn download_all(
    State(state): State<AppState>,
) -> AppResult<impl axum::response::IntoResponse> {
    use crate::error::AppError;
    use axum::http::{header, HeaderValue};
    let dir = state.config.mods_dir.clone();
    let tmp = std::env::temp_dir().join(format!("mods_{}.zip", uuid::Uuid::new_v4()));
    // Build zip into a temp file (blocking); service helper includes cross-process shared lock
    let _guard = state.mods_lock.read().await;
    let dir2 = dir.clone();
    let out = tmp.clone();
    tokio::task::spawn_blocking(move || mods_svc::build_zip_of_dir_to_path(&dir2, &out))
        .await
        .map_err(|e| AppError::Config {
            msg: format!("zip task join error: {}", e),
        })??;
    let file = tokio::fs::File::open(&tmp)
        .await
        .map_err(|e| AppError::Config { msg: e.to_string() })?;
    // Best-effort unlink on unix to avoid temp buildup
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
    resp.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"all_installed_mods.zip\""),
    );
    Ok(resp)
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

async fn update(
    State(state): State<AppState>,
    Json(req): Json<UpdateReq>,
) -> AppResult<Json<mods_svc::InstalledMod>> {
    let m =
        mods_svc::update_from_url(&state, &req.mod_name, &req.download_url, &req.file_name).await?;
    Ok(Json(m))
}

// -------- Mod Portal handlers --------

async fn portal_list() -> AppResult<Json<serde_json::Value>> {
    Ok(Json(portal_svc::list_mods_from_portal().await?))
}

// Provide mod portal info for a specific mod name from the path
async fn portal_info(Path(mod_name): Path<String>) -> AppResult<Json<portal_svc::ModDetails>> {
    let details = portal_svc::mod_info(&mod_name).await?;
    Ok(Json(details))
}

#[derive(serde::Deserialize)]
struct PortalLoginReq {
    username: String,
    password: String,
}

async fn portal_login(
    State(state): State<AppState>,
    Json(req): Json<PortalLoginReq>,
) -> AppResult<Json<serde_json::Value>> {
    portal_svc::login(&state, &req.username, &req.password).await?;
    Ok(Json(serde_json::json!({})))
}

async fn portal_login_status(State(state): State<AppState>) -> AppResult<Json<bool>> {
    Ok(Json(portal_svc::login_status(&state).await?))
}

async fn portal_logout(State(state): State<AppState>) -> AppResult<Json<bool>> {
    Ok(Json(portal_svc::logout(&state).await?))
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

async fn portal_install(
    State(state): State<AppState>,
    Json(req): Json<PortalInstallReq>,
) -> AppResult<Json<serde_json::Value>> {
    let mods =
        portal_svc::install_one(&state, &req.download_url, &req.file_name, &req.mod_name).await?;
    Ok(Json(serde_json::json!({ "mods": mods })))
}

async fn portal_install_multiple(
    State(state): State<AppState>,
    Json(payload): Json<Vec<serde_json::Value>>,
) -> AppResult<Json<serde_json::Value>> {
    // Expect array of { name, version }
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
    let mods = portal_svc::install_multiple(&state, &items).await?;
    Ok(Json(serde_json::json!({ "mods": mods })))
}
