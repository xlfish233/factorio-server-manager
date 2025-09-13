use axum::{
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};

use crate::state::AppState;

async fn serve_index(State(_state): State<AppState>) -> impl IntoResponse {
    let path = std::path::Path::new("./app/index.html");
    match tokio::fs::read_to_string(path).await {
        Ok(html) => Html(html).into_response(),
        Err(_) => Html("<h1>App not found</h1>".to_string()).into_response(),
    }
}

pub fn public_pages() -> Router<AppState> {
    Router::new()
        .route("/login", get(serve_index))
        // Match trailing slash and subpaths like /login/
        .route("/login/{*rest}", get(serve_index))
}

pub fn protected_pages() -> Router<AppState> {
    Router::new()
        .route("/", get(serve_index))
        .route("/saves", get(serve_index))
        .route("/saves/{*rest}", get(serve_index))
        .route("/mods", get(serve_index))
        .route("/mods/{*rest}", get(serve_index))
        .route("/server-settings", get(serve_index))
        .route("/server-settings/{*rest}", get(serve_index))
        .route("/game-settings", get(serve_index))
        .route("/game-settings/{*rest}", get(serve_index))
        .route("/console", get(serve_index))
        .route("/console/{*rest}", get(serve_index))
        .route("/logs", get(serve_index))
        .route("/logs/{*rest}", get(serve_index))
        .route("/user-management", get(serve_index))
        .route("/user-management/{*rest}", get(serve_index))
        .route("/help", get(serve_index))
        .route("/help/{*rest}", get(serve_index))
}
