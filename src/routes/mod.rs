//! HTTP routes wiring. Keep handlers thin; delegate to services.

use axum::{middleware, Router};

use crate::state::AppState;

pub mod auth;
pub mod health;
pub mod misc;
pub mod mod_packs;
pub mod mods;
pub mod saves;
pub mod server;
pub mod settings;
pub mod static_files;
pub mod user;
pub mod ws;

/// Build the application's router tree with shared state.
pub fn build_router(state: AppState) -> Router {
    use axum::routing::get_service;
    use tower_http::services::ServeDir;

    let public = Router::new()
        .merge(health::router())
        .merge(auth::router_public())
        .merge(static_files::public_pages())
        .fallback_service(get_service(ServeDir::new("./app")));

    let protected_api = Router::new()
        .merge(user::router())
        .merge(misc::api_router())
        .merge(saves::router(state.clone()))
        .merge(mods::router(state.clone()))
        .merge(mod_packs::router(state.clone()))
        .merge(server::router(state.clone()))
        .merge(settings::router())
        .merge(auth::router_protected())
        // everything under /api requires auth
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::require_auth,
        ));

    let protected_pages = static_files::protected_pages().route_layer(
        middleware::from_fn_with_state(state.clone(), crate::middleware::require_auth),
    );

    // Protect /ws at root path (before static fallbacks)
    let ws_protected = ws::router().route_layer(middleware::from_fn_with_state(
        state.clone(),
        crate::middleware::require_auth,
    ));

    public
        .nest("/api", protected_api)
        .merge(ws_protected)
        .merge(protected_pages)
        .with_state(state)
}
