use axum::extract::FromRef;
use axum_extra::extract::cookie::{Cookie, Key, SameSite, SignedCookieJar};
use base64::Engine;
use cookie::time::{Duration, OffsetDateTime};
use rand::RngCore;

use crate::config::Config;

pub const AUTH_COOKIE_NAME: &str = "authentication";

pub fn generate_session_id() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn add_session_cookie(jar: SignedCookieJar, cfg: &Config, session_id: &str) -> SignedCookieJar {
    let mut cookie = Cookie::new(AUTH_COOKIE_NAME, session_id.to_string());
    cookie.set_http_only(true);
    cookie.set_path("/");
    cookie.set_same_site(SameSite::Lax);
    cookie.set_secure(cfg.secure);
    jar.add(cookie)
}

pub fn remove_session_cookie(jar: SignedCookieJar, cfg: &Config) -> SignedCookieJar {
    let mut cookie = Cookie::new(AUTH_COOKIE_NAME, "");
    cookie.set_http_only(true);
    cookie.set_path("/");
    cookie.set_same_site(SameSite::Lax);
    cookie.set_secure(cfg.secure);
    cookie.set_expires(OffsetDateTime::UNIX_EPOCH);
    cookie.set_max_age(Duration::seconds(0));
    jar.remove(cookie)
}

pub fn get_session_id_signed(jar: &SignedCookieJar) -> Option<String> {
    jar.get(AUTH_COOKIE_NAME).map(|c| c.value().to_string())
}

use crate::state::AppState;

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
}
