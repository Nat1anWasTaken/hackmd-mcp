//! The user's HackMD API token, in the framework's link store as the secret
//! [`TOKEN`]. The link is `ok`, or `needs_relink` once HackMD rejected the
//! token (the framework flags it).

use chrono::{DateTime, Utc};
use mcp_backbone::{
    Core,
    links::{LinkState, LinkStore, NewLink, token_hash},
};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

pub const TOKEN: &str = "token";

/// The user's HackMD link as the dashboard shows it (`LinkStatus`); `null`
/// when no token is stored.
#[derive(Debug, Serialize)]
pub struct LinkStatus {
    pub status: LinkState,
    pub last_ok_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

pub async fn status(core: &Core, user_id: Uuid) -> sqlx::Result<Option<LinkStatus>> {
    Ok(LinkStore::new(core)
        .status(user_id)
        .await?
        .map(|status| LinkStatus {
            status: status.status,
            last_ok_at: status.last_ok_at,
            last_error: status.last_error,
            updated_at: status.updated_at,
        }))
}

/// Stores `token` as the user's (verified) HackMD token, replacing any other.
pub async fn store(core: &Core, user_id: Uuid, token: &str) -> anyhow::Result<()> {
    LinkStore::new(core)
        .put(
            user_id,
            &NewLink {
                secrets: &[(TOKEN, token)],
                token_hash: token_hash(token),
                expires_at: None,
                metadata: json!({}),
            },
        )
        .await
}
