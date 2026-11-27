//! HackMD as an mcp-backbone service.

use axum::{
    Router,
    routing::{delete, get, post},
};
use mcp_backbone::{
    Access, AppState, Core, Failure, Info, Linked, Refusal, Service, Stop, Tool, links::LinkStore,
};
use serde_json::Value;
use uuid::Uuid;

use crate::{api, auth, links, scope::WorkspaceScope, tools};

pub static INFO: Info = Info {
    slug: "hackmd",
    name: "HackMD",
    version: env!("CARGO_PKG_VERSION"),
    // Served from the dashboard's static files (web/public).
    icon: Some("/hackmd.png"),
    // Kept from before the framework, so signed-in browsers stay signed in.
    cookie_prefix: Some("hmcp"),
    instructions: INSTRUCTIONS,
    allowlist_env: &["ALLOWED_HACKMD_USERS"],
    // Room for a whole note body (HackMD caps notes well below this).
    max_request_bytes: 8 * 1024 * 1024,
};

const INSTRUCTIONS: &str = "\
These tools act as the user's own HackMD account. Use them only as the user asks.

Notes and folders live in a workspace: the user's personal one (the default) or a team's, \
given as {\"kind\": \"team\", \"team_path\": \"...\"}. hackmd_get_me lists the user's teams.

To change a note's text, read it with hackmd_get_note and send a patch against its patch_path \
to hackmd_edit_note. Use hackmd_update_note for metadata, and for replacing a whole body only \
when a patch cannot express the change. Before a large rewrite, consider saving a named \
version with hackmd_create_version so the user can compare or go back.

The user sets this connection's permissions in the dashboard: it may be read-only, limited to \
some tools, or limited to some workspaces. A denied call says why; it is the user's decision, \
not a failure to work around.";

/// What a request acts on, to check against the session's workspace scope.
#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    /// The account as a whole, not one workspace.
    Account,
    /// A workspace: a team's path, or `None` for the personal one.
    Workspace(Option<String>),
    /// A note given by ID alone; its workspace is looked up when it matters.
    Note(String),
}

pub struct HackMdService {
    pub hackmd: hackmd::HackMd,
}

impl Service for HackMdService {
    type Account = hackmd::Account;
    type Error = hackmd::Error;
    type Scope = WorkspaceScope;
    type Target = Target;

    fn info(&self) -> &'static Info {
        &INFO
    }

    fn tools(&self) -> Vec<Tool<Self>> {
        tools::all()
    }

    async fn account(&self, core: &Core, user_id: Uuid) -> anyhow::Result<Access<hackmd::Account>> {
        let Some(link) = LinkStore::new(core).load(user_id).await? else {
            return Ok(Access::NotLinked);
        };
        if !link.is_usable() {
            return Ok(Access::NeedsRelink);
        }
        let token = link.secret(core, links::TOKEN)?;
        Ok(Access::Ready(Linked::new(
            self.hackmd.account(token),
            link.token_hash.clone(),
        )))
    }

    fn classify(&self, error: &hackmd::Error) -> Failure {
        match error {
            hackmd::Error::InvalidToken => Failure::Unauthorized,
            _ => Failure::Other,
        }
    }

    async fn check_target(
        &self,
        account: &hackmd::Account,
        scope: &WorkspaceScope,
        target: Target,
    ) -> Result<(), Refusal<hackmd::Error>> {
        let team = match target {
            Target::Account => return Ok(()),
            Target::Workspace(team) => team,
            Target::Note(note_id) => account
                .note_team(&note_id)
                .await
                .map_err(Refusal::Upstream)?,
        };
        match scope.allows(team.as_deref()) {
            true => Ok(()),
            false => Err(Stop::denied(WorkspaceScope::denial(team.as_deref())).into()),
        }
    }

    async fn link_status(&self, core: &Core, user_id: Uuid) -> anyhow::Result<Value> {
        Ok(serde_json::to_value(links::status(core, user_id).await?)?)
    }

    fn not_linked_message(&self, core: &Core) -> String {
        format!(
            "No HackMD account is linked. Ask the user to sign in to the dashboard at {}.",
            core.config.public_url
        )
    }

    fn relink_message(&self, core: &Core) -> String {
        format!(
            "HackMD rejected the user's API token. Ask the user to sign in to the dashboard at {} \
             with a new HackMD API token.",
            core.config.public_url
        )
    }

    fn routes(&self) -> Router<AppState<Self>> {
        Router::new()
            .route("/auth/hackmd", post(auth::sign_in))
            .route("/api/hackmd", delete(api::disconnect))
            .route("/api/hackmd/teams", get(api::teams))
    }
}
