//! The user's HackMD account: its teams, and disconnecting it.

use axum::{Json, extract::State, http::StatusCode};

use crate::{
    auth::CurrentUser,
    error::{AppError, AppResult},
    links::{self, Unusable},
    state::AppState,
};

/// The teams the user belongs to, live from HackMD, for the workspace scope
/// picker.
pub async fn teams(
    State(state): State<AppState>,
    user: CurrentUser,
) -> AppResult<Json<Vec<hackmd::Team>>> {
    let (account, hash) = match links::account(&state, user.id).await? {
        Ok(account) => account,
        Err(Unusable::NotLinked) => {
            return Err(AppError::Conflict("No HackMD token is linked.".into()));
        }
        Err(Unusable::NeedsRelink) => return Err(AppError::InvalidToken),
    };
    match account.me().await {
        Ok(me) => Ok(Json(me.teams)),
        Err(hackmd::Error::InvalidToken) => {
            links::mark_invalid(&state.db, user.id, &hash).await?;
            Err(AppError::InvalidToken)
        }
        Err(err) => Err(err.into()),
    }
}

/// Deletes the user with their token, sessions, agent connections and call
/// log. Signing in again starts afresh.
pub async fn disconnect(State(state): State<AppState>, user: CurrentUser) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user.id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
