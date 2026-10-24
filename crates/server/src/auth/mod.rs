//! HackMD sign-in for the dashboard and the cookie sessions it creates.

mod hackmd;
mod session;

use axum::{Router, middleware, routing::post};
use axum_extra::extract::cookie::{Cookie, SameSite};

use crate::{api::csrf_guard, config::Config, state::AppState};

pub use session::CurrentUser;
#[cfg(test)]
pub use session::start as start_session;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/auth/hackmd", post(hackmd::sign_in))
        .route("/auth/logout", post(session::logout))
        .layer(middleware::from_fn_with_state(state, csrf_guard))
}

/// An HttpOnly, `SameSite=Lax` cookie, `Secure` when served over https.
fn cookie(
    config: &Config,
    name: &'static str,
    value: String,
    path: &'static str,
    max_age: time::Duration,
) -> Cookie<'static> {
    Cookie::build((name, value))
        .path(path)
        .max_age(max_age)
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(config.secure_cookies())
        .build()
}
