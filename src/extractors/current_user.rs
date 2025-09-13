use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};
use axum_extra::extract::cookie::SignedCookieJar;

use crate::{error::AppError, services::user_service, sessions, state::AppState};

pub struct CurrentUser(pub crate::entity::user::Model);

impl<S> FromRequestParts<S> for CurrentUser
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = AppError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let state = AppState::from_ref(state).clone();
        async move {
            // Extract a signed cookie jar using the app's Key from state
            let signed_jar = SignedCookieJar::from_request_parts(parts, &state)
                .await
                .map_err(|_| AppError::Unauthorized {
                    msg: "missing cookies".into(),
                })?;
            let sid = sessions::get_session_id_signed(&signed_jar).ok_or_else(|| {
                AppError::Unauthorized {
                    msg: "missing session".into(),
                }
            })?;
            let username = state
                .sessions
                .get(&sid)
                .map(|e| e.value().clone())
                .ok_or_else(|| AppError::Unauthorized {
                    msg: "invalid session".into(),
                })?;

            let model = user_service::get_by_username(&state.db, &username)
                .await?
                .ok_or_else(|| AppError::Unauthorized {
                    msg: "user not found".into(),
                })?;
            Ok(CurrentUser(model))
        }
    }
}
