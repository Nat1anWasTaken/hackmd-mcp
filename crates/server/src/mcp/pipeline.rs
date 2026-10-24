//! The path every tool call takes: permission checks, execution and the call
//! log.

use std::time::Instant;

use rmcp::{
    ErrorData,
    model::{CallToolResult, JsonObject},
};
use serde_json::Value;
use uuid::Uuid;

use super::{
    log::{self, NewCall, Status},
    output::{failure, success},
    policy::{self, Denial},
    tools::{self, Call, Target, Tool},
};
use crate::{
    links::{self, Unusable},
    oauth::AgentPrincipal,
    sessions::{self, Permissions, ScopeMode},
    state::AppState,
};

pub const REVOKED: &str = "This connection was disconnected in the dashboard.";

/// Handles `tools/call`. Only an unknown tool, a revoked session or an
/// internal failure is a protocol error; everything else is a tool result the
/// agent can read.
pub async fn call(
    state: &AppState,
    principal: AgentPrincipal,
    name: &str,
    args: JsonObject,
) -> Result<CallToolResult, ErrorData> {
    let tool = tools::find(name)
        .ok_or_else(|| ErrorData::invalid_params(format!("unknown tool: {name}"), None))?;
    let session = load_session(state, principal).await?;
    run(state, principal, session, tool, Value::Object(args))
        .await
        .map_err(internal_error)
}

/// The session's permissions, read fresh on every request so dashboard
/// changes apply immediately.
pub async fn load_session(
    state: &AppState,
    principal: AgentPrincipal,
) -> Result<Permissions, ErrorData> {
    sessions::permissions(&state.db, principal.user_id, principal.session_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| ErrorData::invalid_request(REVOKED, None))
}

fn internal_error(err: impl std::fmt::Debug) -> ErrorData {
    tracing::error!(error = ?err, "serving an MCP request");
    ErrorData::internal_error("internal error", None)
}

async fn run(
    state: &AppState,
    principal: AgentPrincipal,
    session: Permissions,
    tool: &'static Tool,
    args: Value,
) -> anyhow::Result<CallToolResult> {
    let started = Instant::now();
    let request = match (tool.parse)(&args) {
        Ok(request) => request,
        Err(err) => {
            let entry = NewCall::unparsed(principal, tool.name, &args);
            let stop = Stop::error(format!("Invalid arguments: {err}"));
            return refuse(state, &entry, stop, started).await;
        }
    };
    drop(args);
    let entry = NewCall::new(principal, tool.name, &request.log_arguments());
    let (account, token_hash) =
        match admit(state, principal, &session, tool, request.as_ref()).await? {
            Ok(admitted) => admitted,
            Err(stop) => return refuse(state, &entry, stop, started).await,
        };
    // Detached: rmcp drops this request's future when the client goes away,
    // but a change that started must run to its end and be logged.
    let state = state.clone();
    let task = tokio::spawn(async move {
        let id = log::start(&state.db, &entry).await?;
        let outcome = request.run(account).await;
        let (result, logged) =
            settle(&state, principal.user_id, &token_hash, id, outcome, started).await;
        if let Err(err) = logged {
            // The agent still gets the outcome.
            tracing::error!(%id, error = ?err, "logging the outcome of a tool call");
        }
        Ok(result)
    });
    task.await.unwrap_or_else(|err| {
        tracing::error!(tool = tool.name, error = ?err, "tool call task failed");
        Ok(failure("internal error"))
    })
}

/// Why a call ended without running, and how it is logged.
pub struct Stop {
    pub status: Status,
    pub message: String,
}

impl Stop {
    pub fn error(message: String) -> Self {
        Self {
            status: Status::Error,
            message,
        }
    }
}

impl From<Denial> for Stop {
    fn from(denial: Denial) -> Self {
        Self {
            status: Status::Denied,
            message: denial.to_string(),
        }
    }
}

/// The checks every call passes before it runs. Returns the HackMD account
/// to run it with and the hash of its token.
async fn admit(
    state: &AppState,
    principal: AgentPrincipal,
    session: &Permissions,
    tool: &Tool,
    request: &dyn Call,
) -> anyhow::Result<Result<(hackmd::Account, String), Stop>> {
    if let Err(denial) = policy::check_tool(session, tool) {
        return Ok(Err(denial.into()));
    }
    let (account, hash) = match links::account(state, principal.user_id).await? {
        Ok(linked) => linked,
        Err(unusable) => return Ok(Err(Stop::error(unusable_message(state, unusable)))),
    };
    let scope = &session.workspace_scope;
    let team = match request.target() {
        Target::Account => return Ok(Ok((account, hash))),
        Target::Workspace(workspace) => workspace.team_path().map(str::to_owned),
        // Only a restricted scope needs to know where the note lives.
        Target::Note(_) if scope.mode == ScopeMode::All => return Ok(Ok((account, hash))),
        Target::Note(note_id) => match account.note_team(note_id).await {
            Ok(team) => team,
            Err(err) => {
                let message = failure_message(state, principal.user_id, &hash, err).await;
                return Ok(Err(Stop::error(message)));
            }
        },
    };
    Ok(match policy::check_workspace(scope, team.as_deref()) {
        Ok(()) => Ok((account, hash)),
        Err(denial) => Err(denial.into()),
    })
}

fn unusable_message(state: &AppState, unusable: Unusable) -> String {
    let url = &state.config.public_url;
    match unusable {
        Unusable::NotLinked => {
            format!(
                "No HackMD account is linked. Ask the user to sign in to the dashboard at {url}."
            )
        }
        Unusable::NeedsRelink => relink_message(state),
    }
    .to_string()
}

fn relink_message(state: &AppState) -> String {
    format!(
        "HackMD rejected the user's API token. Ask the user to sign in to the dashboard at {} \
         with a new HackMD API token.",
        state.config.public_url
    )
}

/// The message for a failed HackMD request, flagging the token when HackMD
/// rejected it.
async fn failure_message(
    state: &AppState,
    user_id: Uuid,
    token_hash: &str,
    err: hackmd::Error,
) -> String {
    match err {
        hackmd::Error::InvalidToken => {
            if let Err(err) = links::mark_invalid(&state.db, user_id, token_hash).await {
                tracing::error!(%user_id, error = ?err, "flagging a rejected HackMD token");
            }
            relink_message(state)
        }
        other => other.to_string(),
    }
}

/// Logs a call that ends without running.
async fn refuse(
    state: &AppState,
    entry: &NewCall,
    stop: Stop,
    started: Instant,
) -> anyhow::Result<CallToolResult> {
    log::record(
        &state.db,
        entry,
        stop.status,
        &stop.message,
        started.elapsed(),
    )
    .await?;
    Ok(failure(stop.message))
}

/// Records the outcome of a call logged as `running` under `id`.
async fn settle(
    state: &AppState,
    user_id: Uuid,
    token_hash: &str,
    id: Uuid,
    outcome: hackmd::Result<Value>,
    started: Instant,
) -> (CallToolResult, sqlx::Result<log::ToolCall>) {
    match outcome {
        Ok(value) => {
            if let Err(err) = links::mark_ok(&state.db, user_id, token_hash).await {
                tracing::warn!(%user_id, error = ?err, "recording a working HackMD token");
            }
            let (result, logged) = success(value);
            let logged = log::finish(
                &state.db,
                id,
                Status::Ok,
                Some(logged),
                None,
                started.elapsed(),
            )
            .await;
            (result, logged)
        }
        Err(err) => {
            let message = failure_message(state, user_id, token_hash, err).await;
            let logged = log::finish(
                &state.db,
                id,
                Status::Error,
                None,
                Some(&message),
                started.elapsed(),
            )
            .await;
            (failure(message), logged)
        }
    }
}
