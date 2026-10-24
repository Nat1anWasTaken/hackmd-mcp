use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, Icon, Implementation, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool as Definition,
    },
    service::RequestContext,
};

use super::{pipeline, policy, tools};
use crate::{oauth::AgentPrincipal, state::AppState};

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

/// The MCP server for one request. Stateless: every request is authenticated
/// on its own and reads its session's permissions fresh.
pub struct HackMdServer {
    state: AppState,
}

impl HackMdServer {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

impl ServerHandler for HackMdServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("hackmd", env!("CARGO_PKG_VERSION"))
                    .with_title("HackMD")
                    .with_icons(vec![
                        // Served from the dashboard's static files (web/public).
                        Icon::new(format!("{}/hackmd.png", self.state.config.public_url))
                            .with_mime_type("image/png")
                            .with_sizes(vec!["512x512".into()]),
                    ]),
            )
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let session = pipeline::load_session(&self.state, principal(&context)?).await?;
        let listed = tools::all()
            .filter(|tool| policy::check_tool(&session, tool).is_ok())
            .map(tools::Tool::definition)
            .collect();
        Ok(ListToolsResult::with_all_items(listed))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let principal = principal(&context)?;
        let args = request.arguments.unwrap_or_default();
        pipeline::call(&self.state, principal, &request.name, args)
            .await
            .map(Into::into)
    }

    fn get_tool(&self, name: &str) -> Option<Definition> {
        tools::find(name).map(tools::Tool::definition)
    }
}

/// The agent the bearer middleware authenticated; rmcp passes the HTTP
/// request parts through.
fn principal(context: &RequestContext<RoleServer>) -> Result<AgentPrincipal, ErrorData> {
    context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<AgentPrincipal>())
        .copied()
        .ok_or_else(|| ErrorData::internal_error("request is not authenticated", None))
}
