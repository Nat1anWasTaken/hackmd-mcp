//! Agent sessions: one connected MCP client per user, and the permissions the
//! user granted it. Shared by the dashboard API and the MCP layer.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{FromRow, PgPool, types::Json};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AgentSession {
    pub id: Uuid,
    pub name: String,
    pub client_id: String,
    pub client_name: Option<String>,
    pub origin: Option<String>,
    pub enabled: bool,
    pub read_only: bool,
    /// `None` means every tool.
    pub allowed_tools: Option<Vec<String>>,
    #[sqlx(json)]
    pub workspace_scope: WorkspaceScope,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Which HackMD workspaces a session may touch. `teams` are snapshots taken
/// when the user picked them, so the dashboard can render them without
/// asking HackMD.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceScope {
    pub mode: ScopeMode,
    /// With `allow`: whether the personal workspace is allowed.
    #[serde(default = "yes")]
    pub personal: bool,
    /// With `allow`: the allowed teams.
    #[serde(default)]
    pub teams: Vec<ScopedTeam>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScopeMode {
    /// Every workspace the HackMD account can reach.
    All,
    /// Only the listed ones.
    Allow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedTeam {
    /// The team path, as HackMD API routes use it.
    pub path: String,
    pub name: String,
}

impl WorkspaceScope {
    /// Whether the scope allows the team with `team_path`, or the personal
    /// workspace for `None`.
    pub fn allows(&self, team_path: Option<&str>) -> bool {
        match (self.mode, team_path) {
            (ScopeMode::All, _) => true,
            (ScopeMode::Allow, None) => self.personal,
            (ScopeMode::Allow, Some(path)) => self
                .teams
                .iter()
                .any(|team| team.path.eq_ignore_ascii_case(path)),
        }
    }
}

/// What a session may do, read on every MCP request so dashboard changes
/// apply immediately.
#[derive(Debug, Clone, FromRow)]
pub struct Permissions {
    pub enabled: bool,
    pub read_only: bool,
    /// `None` means every tool.
    pub allowed_tools: Option<Vec<String>>,
    #[sqlx(json)]
    pub workspace_scope: WorkspaceScope,
}

/// A partial update from `PATCH /api/sessions/:id`; absent fields are left alone.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionPatch {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub read_only: Option<bool>,
    /// `Some(None)` (JSON `null`) grants every tool.
    #[serde(default, deserialize_with = "present")]
    pub allowed_tools: Option<Option<Vec<String>>>,
    pub workspace_scope: Option<WorkspaceScope>,
}

/// Maps a present field to `Some`, so `null` becomes `Some(None)` and only an
/// absent field (via `#[serde(default)]`) stays `None`.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// Trims a user-supplied session name and checks it is reasonable.
pub fn normalize_name(name: &str) -> AppResult<String> {
    const MAX_CHARS: usize = 100;
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_CHARS {
        return Err(AppError::BadRequest(format!(
            "name must be 1 to {MAX_CHARS} characters"
        )));
    }
    Ok(name.to_owned())
}

const SELECT_ACTIVE: &str = "
    SELECT s.id, s.name, s.client_id, c.client_name, s.origin, s.enabled, s.read_only,
           s.allowed_tools, s.workspace_scope, s.created_at, s.last_used_at
    FROM agent_sessions s
    JOIN oauth_clients c ON c.client_id = s.client_id
    WHERE s.user_id = $1 AND s.revoked_at IS NULL AND ($2::uuid IS NULL OR s.id = $2)
    ORDER BY s.created_at DESC";

/// The user's non-revoked sessions, newest first.
pub async fn list_active(db: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<AgentSession>> {
    sqlx::query_as(SELECT_ACTIVE)
        .bind(user_id)
        .bind(None::<Uuid>)
        .fetch_all(db)
        .await
}

/// A non-revoked session owned by `user_id`.
pub async fn find(db: &PgPool, user_id: Uuid, id: Uuid) -> sqlx::Result<Option<AgentSession>> {
    sqlx::query_as(SELECT_ACTIVE)
        .bind(user_id)
        .bind(id)
        .fetch_optional(db)
        .await
}

/// The permissions of a non-revoked session owned by `user_id`.
pub async fn permissions(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> sqlx::Result<Option<Permissions>> {
    sqlx::query_as(
        "SELECT enabled, read_only, allowed_tools, workspace_scope
         FROM agent_sessions WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

/// Applies `patch` to a non-revoked session. Returns whether it exists.
pub async fn update(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
    patch: &SessionPatch,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE agent_sessions SET
            name = COALESCE($3, name),
            enabled = COALESCE($4, enabled),
            read_only = COALESCE($5, read_only),
            allowed_tools = CASE WHEN $6 THEN $7 ELSE allowed_tools END,
            workspace_scope = COALESCE($8, workspace_scope)
         WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .bind(&patch.name)
    .bind(patch.enabled)
    .bind(patch.read_only)
    .bind(patch.allowed_tools.is_some())
    .bind(patch.allowed_tools.as_ref().and_then(Option::as_ref))
    .bind(patch.workspace_scope.as_ref().map(Json))
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Revokes a session and every credential issued to it. Its tool-call log is
/// kept. Returns whether an active session was revoked.
pub async fn revoke(db: &PgPool, user_id: Uuid, id: Uuid) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    let revoked = sqlx::query(
        "UPDATE agent_sessions SET revoked_at = now()
         WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if revoked {
        for sql in [
            "DELETE FROM oauth_access_tokens WHERE agent_session_id = $1",
            "DELETE FROM oauth_refresh_tokens WHERE agent_session_id = $1",
            "DELETE FROM oauth_codes WHERE agent_session_id = $1",
        ] {
            sqlx::query(sql).bind(id).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(revoked)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn allow(personal: bool, teams: &[&str]) -> WorkspaceScope {
        WorkspaceScope {
            mode: ScopeMode::Allow,
            personal,
            teams: teams
                .iter()
                .map(|path| ScopedTeam {
                    path: (*path).into(),
                    name: path.to_uppercase(),
                })
                .collect(),
        }
    }

    #[test]
    fn workspace_scope_allows() {
        let all: WorkspaceScope =
            serde_json::from_value(json!({"mode": "all", "personal": true, "teams": []})).unwrap();
        assert!(all.allows(None) && all.allows(Some("any")));
        assert!(allow(true, &[]).allows(None));
        assert!(!allow(true, &[]).allows(Some("core")));
        assert!(!allow(false, &["core"]).allows(None));
        assert!(allow(false, &["core"]).allows(Some("Core")));
    }

    #[test]
    fn workspace_scope_round_trips_the_database_default() {
        let parsed: WorkspaceScope =
            serde_json::from_value(json!({"mode":"all","personal":true,"teams":[]})).unwrap();
        assert_eq!(parsed.mode, ScopeMode::All);
        let value = json!({"mode": "allow", "personal": false, "teams": [{"path": "core", "name": "Core"}]});
        let parsed: WorkspaceScope = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap(), value);
        assert!(serde_json::from_value::<WorkspaceScope>(json!({"mode": "deny"})).is_err());
    }

    #[test]
    fn patch_distinguishes_absent_from_null() {
        let patch: SessionPatch = serde_json::from_value(json!({"read_only": false})).unwrap();
        assert_eq!(patch.allowed_tools, None);
        let patch: SessionPatch = serde_json::from_value(json!({"allowed_tools": null})).unwrap();
        assert_eq!(patch.allowed_tools, Some(None));
        let patch: SessionPatch =
            serde_json::from_value(json!({"allowed_tools": ["hackmd_get_note"]})).unwrap();
        assert_eq!(
            patch.allowed_tools,
            Some(Some(vec!["hackmd_get_note".into()]))
        );
        assert!(serde_json::from_value::<SessionPatch>(json!({"bogus": 1})).is_err());
    }
}
