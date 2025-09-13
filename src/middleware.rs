use axum::{
    body::Body,
    extract::State,
    http::{header::LOCATION, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::SignedCookieJar;

use crate::{sessions, state::AppState};
use tracing::{debug, info, warn};

pub async fn require_auth(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = req.uri().path().to_string();
    // Build a signed cookie jar from request headers
    let signed = SignedCookieJar::from_headers(req.headers(), state.cookie_key.clone());

    if let Some(sid) = sessions::get_session_id_signed(&signed) {
        if state.sessions.get(&sid).is_some() {
            // Optionally: check user existence in DB here
            debug!(%path, "Request authorized");
            return Ok(next.run(req).await);
        }
    }

    // Mirror Go behavior: if configured, redirect all unauthenticated
    // requests to /login (even under /api). Otherwise return 401.
    if state.config.redirect_on_unauth {
        info!(%path, "Unauthorized request, redirecting to /login");
        let mut resp = Response::new(Body::empty());
        *resp.status_mut() = StatusCode::SEE_OTHER;
        resp.headers_mut()
            .insert(LOCATION, axum::http::HeaderValue::from_static("/login"));
        Ok(resp)
    } else {
        warn!(%path, "Unauthorized request, returning 401");
        Err(StatusCode::UNAUTHORIZED)
    }
}

pub async fn server_off(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let running = { state.server.read().await.running };
    if running {
        Err(StatusCode::LOCKED)
    } else {
        Ok(next.run(req).await)
    }
}
