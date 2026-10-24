//! Per-user HackMD API tokens: sealed storage and health bookkeeping.
//!
//! The token is the user's identity and their agents' access alike. It is
//! sealed with the user's id as associated data, so a row cannot be moved to
//! another user, and only ever unsealed to make a HackMD request.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgExecutor, PgPool};
use uuid::Uuid;

use crate::{config::Config, crypto::sealed, state::AppState};

/// `last_ok_at` is bumped at most this often.
const OK_THROTTLE_SECS: i32 = 60;

/// The user's HackMD link as the dashboard shows it (`LinkStatus`).
#[derive(Debug, Serialize, FromRow)]
pub struct LinkStatus {
    /// `ok`, or `needs_relink` once HackMD rejected the token.
    pub status: String,
    pub last_ok_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

pub async fn status(db: &PgPool, user_id: Uuid) -> sqlx::Result<Option<LinkStatus>> {
    sqlx::query_as(
        "SELECT status, last_ok_at, last_error, updated_at FROM hackmd_links WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
}

/// Hex SHA-256 of a token, to tell tokens apart without unsealing them.
pub fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Stores `token` as the user's (verified) HackMD token, replacing any other.
pub async fn store(
    db: impl PgExecutor<'_>,
    config: &Config,
    user_id: Uuid,
    token: &str,
) -> anyhow::Result<()> {
    let sealed = sealed::seal(&config.encryption_key, user_id.as_bytes(), token.as_bytes())?;
    sqlx::query(
        "INSERT INTO hackmd_links (user_id, token_sealed, token_hash, last_ok_at)
         VALUES ($1, $2, $3, now())
         ON CONFLICT (user_id) DO UPDATE SET token_sealed = EXCLUDED.token_sealed,
             token_hash = EXCLUDED.token_hash, status = 'ok', last_ok_at = now(),
             last_error = NULL, updated_at = now()",
    )
    .bind(user_id)
    .bind(sealed)
    .bind(token_hash(token))
    .execute(db)
    .await?;
    Ok(())
}

/// Why a user's HackMD account cannot be used.
#[derive(Debug)]
pub enum Unusable {
    NotLinked,
    /// HackMD rejected the token; the user must paste a new one.
    NeedsRelink,
}

/// The user's HackMD account handle and the hash of the token it uses.
pub async fn account(
    state: &AppState,
    user_id: Uuid,
) -> anyhow::Result<Result<(hackmd::Account, String), Unusable>> {
    let row: Option<(Vec<u8>, String, String)> = sqlx::query_as(
        "SELECT token_sealed, token_hash, status FROM hackmd_links WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;
    let Some((sealed, hash, status)) = row else {
        return Ok(Err(Unusable::NotLinked));
    };
    if status != "ok" {
        return Ok(Err(Unusable::NeedsRelink));
    }
    let token = sealed::open(&state.config.encryption_key, user_id.as_bytes(), &sealed)
        .map_err(|err| err.context("unsealing a HackMD token"))?;
    let token = String::from_utf8(token)?;
    Ok(Ok((state.hackmd.account(token), hash)))
}

/// Records that the token with `hash` worked, at most once a minute.
pub async fn mark_ok(db: &PgPool, user_id: Uuid, hash: &str) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE hackmd_links SET last_ok_at = now(), last_error = NULL
         WHERE user_id = $1 AND token_hash = $2 AND status = 'ok'
           AND (last_ok_at IS NULL OR last_ok_at < now() - make_interval(secs => $3))",
    )
    .bind(user_id)
    .bind(hash)
    .bind(OK_THROTTLE_SECS)
    .execute(db)
    .await?;
    Ok(())
}

/// Flags the token with `hash` as rejected by HackMD. A newer token stored
/// meanwhile is left alone.
pub async fn mark_invalid(db: &PgPool, user_id: Uuid, hash: &str) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE hackmd_links SET status = 'needs_relink',
             last_error = 'HackMD rejected the API token', updated_at = now()
         WHERE user_id = $1 AND token_hash = $2",
    )
    .bind(user_id)
    .bind(hash)
    .execute(db)
    .await?;
    Ok(())
}
