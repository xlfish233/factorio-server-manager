use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use axum_extra::extract::cookie::SignedCookieJar;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::services::user_service;
use crate::sessions;
use crate::state::AppState;
use tracing::{info, warn};

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct UserResponse {
    pub username: String,
    pub password: String,
    pub role: String,
    pub email: String,
}

pub async fn login(
    jar: SignedCookieJar,
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> AppResult<(SignedCookieJar, Json<UserResponse>)> {
    let ok = user_service::check_password(&state.db, &req.username, &req.password).await?;
    if !ok {
        warn!(user=%req.username, "Login failed: wrong password");
        return Err(AppError::Unauthorized {
            msg: format!("Password for user {} wrong", req.username),
        });
    }

    // Create session and set cookie
    let sid = sessions::generate_session_id();
    state.sessions.insert(sid.clone(), req.username.clone());
    let jar = sessions::add_session_cookie(jar, &state.config, &sid);
    info!(user=%req.username, secure=%state.config.secure, "Login success; session established");

    // Return user without password
    let user = user_service::get_by_username(&state.db, &req.username)
        .await?
        .ok_or_else(|| AppError::Config {
            msg: "user missing after login".into(),
        })?;
    let email = user.email.unwrap_or_default();
    Ok((
        jar,
        Json(UserResponse {
            username: user.username,
            password: "".into(),
            role: user.role,
            email,
        }),
    ))
}

pub async fn logout(
    jar: SignedCookieJar,
    State(state): State<AppState>,
) -> AppResult<(SignedCookieJar, String)> {
    if let Some(sid) = sessions::get_session_id_signed(&jar) {
        state.sessions.remove(&sid);
    }
    let jar = sessions::remove_session_cookie(jar, &state.config);
    info!("Logout success; session removed");
    Ok((jar, "User logged out successfully.".into()))
}

pub fn router_public() -> Router<AppState> {
    Router::new().route("/api/login", post(login))
}

pub fn router_protected() -> Router<AppState> {
    Router::new().route("/logout", get(logout))
}
