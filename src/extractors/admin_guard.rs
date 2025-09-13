use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};

use crate::{error::AppError, extractors::current_user::CurrentUser, state::AppState};

pub struct AdminGuard;

#[allow(clippy::manual_async_fn)]
impl<S> FromRequestParts<S> for AdminGuard
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = AppError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            let CurrentUser(user) = CurrentUser::from_request_parts(parts, state).await?;
            if user.role == "admin" {
                Ok(AdminGuard)
            } else {
                Err(AppError::Forbidden {
                    msg: "admin required".into(),
                })
            }
        }
    }
}
