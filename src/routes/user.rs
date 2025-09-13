use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;

use crate::error::AppResult;
use crate::extractors::admin_guard::AdminGuard;
use crate::extractors::current_user::CurrentUser;
use crate::extractors::current_username::CurrentUsername;
use crate::services::user_service;
use crate::state::AppState;

#[derive(Serialize)]
pub struct UserDto {
    pub username: String,
    pub role: String,
    pub email: Option<String>,
}

async fn current_user(CurrentUser(user): CurrentUser) -> AppResult<Json<UserDto>> {
    Ok(Json(UserDto {
        username: user.username,
        role: user.role,
        email: user.email,
    }))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/user/status", get(current_user))
        .route("/user/list", get(list_users))
        .route("/user/add", post(add_user))
        .route("/user/remove", post(remove_user))
        .route("/user/password", post(change_password))
}

async fn list_users(
    _admin: AdminGuard,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<UserDto>>> {
    let list = user_service::list(&state.db).await?;
    let list = list
        .into_iter()
        .map(|u| UserDto {
            username: u.username,
            role: u.role,
            email: u.email,
        })
        .collect();
    Ok(Json(list))
}

#[derive(serde::Deserialize)]
struct AddUserReq {
    username: String,
    password: String,
    role: String,
    #[serde(default)]
    email: Option<String>,
}

async fn add_user(
    _admin: AdminGuard,
    State(state): State<AppState>,
    Json(req): Json<AddUserReq>,
) -> AppResult<String> {
    user_service::add(
        &state.db,
        &req.username,
        &req.password,
        &req.role,
        req.email,
    )
    .await?;
    Ok(format!("User: {} successfully added.", req.username))
}

#[derive(serde::Deserialize)]
struct RemoveUserReq {
    username: String,
}

async fn remove_user(
    _admin: AdminGuard,
    State(state): State<AppState>,
    Json(req): Json<RemoveUserReq>,
) -> AppResult<String> {
    user_service::remove(&state.db, &req.username).await?;
    Ok(format!("User: {} successfully removed.", req.username))
}

#[derive(serde::Deserialize)]
struct ChangePasswordReq {
    old_password: String,
    new_password: String,
    new_password_confirmation: String,
}

async fn change_password(
    CurrentUsername(username): CurrentUsername,
    State(state): State<AppState>,
    Json(req): Json<ChangePasswordReq>,
) -> AppResult<Json<bool>> {
    let ok = user_service::change_password(
        &state.db,
        &username,
        &req.old_password,
        &req.new_password,
        &req.new_password_confirmation,
    )
    .await?;
    Ok(Json(ok))
}
