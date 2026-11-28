//! Dashboard endpoints of HackMD's own.

use axum::{Json, extract::State, http::StatusCode};
use mcp_backbone::{AppResult, AppState, auth::CurrentUser};

use crate::service::HackMdService;

/// The teams the user belongs to, live from HackMD, for the workspace scope
/// picker.
pub async fn teams(
    State(state): State<AppState<HackMdService>>,
    user: CurrentUser,
) -> AppResult<Json<Vec<hackmd::Team>>> {
    let me = state
        .call(user.id, |account| async move { account.me().await })
        .await?;
    Ok(Json(me.teams))
}

/// Deletes the user with their token, sessions, agent connections and call
/// log. Signing in again starts afresh.
pub async fn disconnect(
    State(state): State<AppState<HackMdService>>,
    user: CurrentUser,
) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user.id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
