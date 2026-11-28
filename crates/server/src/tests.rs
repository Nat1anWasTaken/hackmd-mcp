//! The `/mcp` endpoint driven over HTTP against a test database and a fake
//! HackMD API that records what it was asked.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, Method, Request, StatusCode, header},
    response::{IntoResponse, Response as AxumResponse},
    routing::get,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use mcp_backbone::AppState;

use crate::{
    service::HackMdService,
    testing::{self, PUBLIC_URL, Response, dashboard_request, send},
};

const HOST: &str = "mcp.example.test";
const TOKEN: &str = "hackmd-token";

/// What the fake HackMD API holds and saw.
#[derive(Default)]
struct Upstream {
    /// Note bodies by `(team path or "", note id)`.
    notes: Mutex<HashMap<(String, String), String>>,
    /// Every write: `(method, path, body)`.
    writes: Mutex<Vec<(String, String, Value)>>,
    /// Query strings of version listings.
    queries: Mutex<Vec<HashMap<String, String>>>,
}

type Shared = Arc<Upstream>;

fn authorized(headers: &HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .is_some_and(|value| value == format!("Bearer {TOKEN}").as_str())
}

fn note_json(team: &str, id: &str, content: &str) -> Value {
    json!({
        "id": id,
        "shortId": format!("s-{id}"),
        "title": format!("Note {id}"),
        "description": "",
        "tags": ["tag"],
        "createdAt": 1,
        "lastChangedAt": if id == "newer" { 9 } else { 2 },
        "teamPath": if team.is_empty() { Value::Null } else { json!(team) },
        "folderPaths": [],
        "content": content,
    })
}

async fn fake_hackmd(upstream: Shared) -> String {
    async fn me(headers: HeaderMap) -> AxumResponse {
        if !authorized(&headers) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        Json(json!({
            "id": "hm-7", "name": "User 7", "userPath": "user7",
            "teams": [{ "path": "core", "name": "Core", "logo": null }]
        }))
        .into_response()
    }

    async fn list(State(up): State<Shared>, headers: HeaderMap) -> AxumResponse {
        if !authorized(&headers) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        let notes = up.notes.lock().unwrap();
        let list: Vec<Value> = notes
            .iter()
            .filter(|((team, _), _)| team.is_empty())
            .map(|((team, id), content)| note_json(team, id, content))
            .collect();
        Json(list).into_response()
    }

    async fn note(
        up: &Upstream,
        method: Method,
        team: &str,
        id: &str,
        body: Value,
    ) -> AxumResponse {
        let key = (team.to_owned(), id.to_owned());
        let path = if team.is_empty() {
            format!("/notes/{id}")
        } else {
            format!("/teams/{team}/notes/{id}")
        };
        match method {
            Method::GET => match up.notes.lock().unwrap().get(&key) {
                Some(content) => Json(note_json(team, id, content)).into_response(),
                None => (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "Note not found" })),
                )
                    .into_response(),
            },
            Method::PATCH => {
                if let Some(content) = body["content"].as_str() {
                    up.notes.lock().unwrap().insert(key, content.to_owned());
                }
                up.writes.lock().unwrap().push(("PATCH".into(), path, body));
                StatusCode::ACCEPTED.into_response()
            }
            _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
        }
    }

    async fn personal(
        State(up): State<Shared>,
        method: Method,
        headers: HeaderMap,
        Path(id): Path<String>,
        body: Option<Json<Value>>,
    ) -> AxumResponse {
        if !authorized(&headers) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        // Like HackMD, personal note routes also serve team notes by ID.
        let team = up
            .notes
            .lock()
            .unwrap()
            .keys()
            .find(|(_, note)| *note == id)
            .map(|(team, _)| team.clone())
            .unwrap_or_default();
        let team = if method == Method::GET {
            team
        } else {
            String::new()
        };
        note(
            &up,
            method,
            &team,
            &id,
            body.map(|b| b.0).unwrap_or_default(),
        )
        .await
    }

    async fn team(
        State(up): State<Shared>,
        method: Method,
        headers: HeaderMap,
        Path((team, id)): Path<(String, String)>,
        body: Option<Json<Value>>,
    ) -> AxumResponse {
        if !authorized(&headers) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        note(
            &up,
            method,
            &team,
            &id,
            body.map(|b| b.0).unwrap_or_default(),
        )
        .await
    }

    async fn versions(
        State(up): State<Shared>,
        method: Method,
        Path(id): Path<String>,
        Query(query): Query<HashMap<String, String>>,
        body: Option<Json<Value>>,
    ) -> AxumResponse {
        match method {
            Method::GET => {
                up.queries.lock().unwrap().push(query);
                Json(json!({
                    "data": [{ "id": "v1", "name": null, "content_available": true }],
                    "meta": { "total": 1, "page": 1, "limit": 50, "total_pages": 1 }
                }))
                .into_response()
            }
            _ => {
                let body = body.map(|b| b.0).unwrap_or_default();
                if body["name"] == "same" {
                    return (
                        StatusCode::CONFLICT,
                        Json(json!({ "error": "no_content_change" })),
                    )
                        .into_response();
                }
                up.writes.lock().unwrap().push((
                    method.to_string(),
                    format!("/notes/{id}/versions"),
                    body.clone(),
                ));
                (
                    StatusCode::CREATED,
                    Json(json!({ "id": "v2", "name": body["name"] })),
                )
                    .into_response()
            }
        }
    }

    let app = Router::new()
        .route("/me", get(me))
        .route("/notes", get(list))
        .route("/notes/{id}", get(personal).patch(personal))
        .route("/teams/{team}/notes/{id}", get(team).patch(team))
        .route(
            "/notes/{id}/versions",
            get(versions).post(versions).patch(versions),
        )
        .with_state(upstream);
    testing::serve(app).await
}

struct Harness {
    state: AppState<HackMdService>,
    app: Router,
    user_id: Uuid,
    cookie: String,
    session_id: Uuid,
    token: String,
    upstream: Shared,
}

impl Harness {
    /// A signed-in user with an agent session, but no HackMD token linked.
    async fn unlinked(db: PgPool) -> Self {
        let upstream = Shared::default();
        let state = testing::state_with_hackmd(db, &fake_hackmd(upstream.clone()).await).await;
        let (user_id, cookie) = testing::sign_in(&state, 7).await;
        let session_id = testing::agent_session(&state, user_id).await;
        let token = testing::access_token(&state, session_id).await;
        Self {
            app: mcp_backbone::router(state.clone()),
            state,
            user_id,
            cookie,
            session_id,
            token,
            upstream,
        }
    }

    /// [`Self::unlinked`] with a working HackMD token linked.
    async fn new(db: PgPool) -> Self {
        let harness = Self::unlinked(db).await;
        testing::link(&harness.state, harness.user_id, TOKEN).await;
        harness
    }

    fn put_note(&self, team: &str, id: &str, content: &str) {
        self.upstream
            .notes
            .lock()
            .unwrap()
            .insert((team.into(), id.into()), content.into());
    }

    fn writes(&self) -> Vec<(String, String, Value)> {
        self.upstream.writes.lock().unwrap().clone()
    }

    async fn post_mcp(&self, body: Value, bearer: Option<&str>) -> Response {
        let mut request = Request::post("/mcp")
            .header(header::HOST, HOST)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream");
        if let Some(token) = bearer {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        send(
            &self.app,
            request.body(Body::from(body.to_string())).unwrap(),
        )
        .await
    }

    /// A JSON-RPC request; returns the whole response message.
    async fn rpc(&self, method: &str, params: Value) -> Value {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        let response = self.post_mcp(body, Some(&self.token)).await;
        assert_eq!(response.status, StatusCode::OK, "{method}");
        response.json()
    }

    /// Calls a tool; returns its `CallToolResult`.
    async fn call(&self, tool: &str, arguments: Value) -> Value {
        let response = self
            .rpc(
                "tools/call",
                json!({ "name": tool, "arguments": arguments }),
            )
            .await;
        response["result"].clone()
    }

    async fn tool_names(&self) -> Vec<String> {
        let response = self.rpc("tools/list", json!({})).await;
        response["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect()
    }

    async fn patch_session(&self, patch: Value) {
        let uri = format!("/api/sessions/{}", self.session_id);
        let request = dashboard_request(Method::PATCH, &uri, &self.cookie, Some(&patch));
        assert_eq!(send(&self.app, request).await.status, StatusCode::OK);
    }

    async fn tool_calls(&self) -> Value {
        let request = dashboard_request(Method::GET, "/api/tool-calls", &self.cookie, None);
        let response = send(&self.app, request).await;
        assert_eq!(response.status, StatusCode::OK);
        response.json()
    }
}

fn text(result: &Value) -> &str {
    result["content"][0]["text"].as_str().unwrap()
}

/// The JSON a successful tool result carries.
fn json_of(result: &Value) -> Value {
    assert_eq!(result["isError"], false, "{result}");
    serde_json::from_str(text(result)).unwrap()
}

const PATCH: &str =
    "*** Begin Patch\n*** Update File: notes/a.md\n@@\n title\n-old\n+new\n*** End Patch";

#[sqlx::test(migrations = false)]
async fn initialize_and_tools_list(db: PgPool) {
    let harness = Harness::new(db).await;
    let initialize = harness
        .rpc(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1" },
            }),
        )
        .await;
    let info = &initialize["result"];
    assert_eq!(info["serverInfo"]["name"], "hackmd");
    assert_eq!(
        info["serverInfo"]["icons"][0]["src"],
        format!("{PUBLIC_URL}/hackmd.png")
    );
    assert!(info["capabilities"]["tools"].is_object());
    assert!(
        info["instructions"]
            .as_str()
            .unwrap()
            .contains("hackmd_edit_note")
    );

    assert_eq!(harness.tool_names().await.len(), 16);
    harness.patch_session(json!({ "read_only": true })).await;
    let listed = harness.tool_names().await;
    assert_eq!(listed.len(), 7);
    assert!(!listed.iter().any(|name| name == "hackmd_edit_note"));

    harness
        .patch_session(json!({
            "read_only": false,
            "allowed_tools": ["hackmd_get_note", "hackmd_edit_note"],
        }))
        .await;
    assert_eq!(
        harness.tool_names().await,
        ["hackmd_get_note", "hackmd_edit_note"]
    );
    harness.patch_session(json!({ "enabled": false })).await;
    assert_eq!(harness.tool_names().await, Vec::<String>::new());
    harness.patch_session(json!({ "enabled": true })).await;
    let denied = harness.call("hackmd_get_me", json!({})).await;
    assert_eq!(denied["isError"], true);
    assert!(text(&denied).contains("not allowed this connection to use hackmd_get_me"));

    let unknown = harness
        .rpc("tools/call", json!({ "name": "nope", "arguments": {} }))
        .await;
    assert_eq!(unknown["error"]["code"], -32602);
}

#[sqlx::test(migrations = false)]
async fn missing_bearer_gets_the_resource_metadata_challenge(db: PgPool) {
    let harness = Harness::new(db).await;
    let response = harness
        .post_mcp(
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
            None,
        )
        .await;
    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    let challenge = response.headers[header::WWW_AUTHENTICATE].to_str().unwrap();
    assert!(
        challenge.contains(&format!(
            "resource_metadata=\"{PUBLIC_URL}/.well-known/oauth-protected-resource/mcp\""
        )),
        "{challenge}"
    );
}

#[sqlx::test(migrations = false)]
async fn lists_reads_and_edits_notes(db: PgPool) {
    let harness = Harness::new(db).await;
    harness.put_note("", "a", "title\nold\n");
    harness.put_note("", "newer", "x\n");

    let list = json_of(
        &harness
            .call("hackmd_list_notes", json!({ "limit": 1 }))
            .await,
    );
    assert_eq!(list["total"], 2);
    assert_eq!(list["notes"][0]["id"], "newer", "last changed first");
    assert_eq!(list["notes"][0]["patch_path"], "notes/newer.md");
    let found = json_of(
        &harness
            .call("hackmd_list_notes", json!({ "query": "s-a" }))
            .await,
    );
    assert_eq!(found["total"], 1);

    let note = harness
        .call("hackmd_get_note", json!({ "note_id": "a" }))
        .await;
    assert_eq!(note["structuredContent"]["patch_path"], "notes/a.md");
    assert_eq!(note["structuredContent"]["content"], "title\nold\n");

    let edited = json_of(
        &harness
            .call(
                "hackmd_edit_note",
                json!({ "note_id": "a", "patch": PATCH }),
            )
            .await,
    );
    assert_eq!(edited["changed"], true);
    assert_eq!(edited["content"], "title\nnew\n");
    assert_eq!(
        harness.writes(),
        [(
            "PATCH".to_owned(),
            "/notes/a".to_owned(),
            json!({ "content": "title\nnew\n" })
        )]
    );

    // Applying the same patch again fails on context; a no-op patch writes
    // nothing.
    let again = harness
        .call(
            "hackmd_edit_note",
            json!({ "note_id": "a", "patch": PATCH }),
        )
        .await;
    assert_eq!(again["isError"], true);
    assert!(text(&again).contains("context was not found"), "{again}");
    let noop = "*** Begin Patch\n*** Update File: notes/a.md\n@@\n-new\n+new\n*** End Patch";
    let unchanged = json_of(
        &harness
            .call("hackmd_edit_note", json!({ "note_id": "a", "patch": noop }))
            .await,
    );
    assert_eq!(unchanged["changed"], false);
    assert_eq!(harness.writes().len(), 1);

    let calls = harness.tool_calls().await;
    let statuses: Vec<_> = calls["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|call| call["status"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, ["ok", "error", "ok", "ok", "ok", "ok"]);
    let status = crate::links::status(&harness.state, harness.user_id)
        .await
        .unwrap()
        .unwrap();
    assert!(status.last_ok_at.is_some());
}

#[sqlx::test(migrations = false)]
async fn routes_team_workspaces(db: PgPool) {
    let harness = Harness::new(db).await;
    harness.put_note("core", "t1", "title\nold\n");
    let team = json!({ "kind": "team", "team_path": "core" });

    let note = harness
        .call(
            "hackmd_get_note",
            json!({ "workspace": team, "note_id": "t1" }),
        )
        .await;
    assert_eq!(
        note["structuredContent"]["patch_path"],
        "teams/core/notes/t1.md"
    );

    let patch = PATCH.replace("notes/a.md", "teams/core/notes/t1.md");
    let edited = harness
        .call(
            "hackmd_edit_note",
            json!({ "workspace": team, "note_id": "t1", "patch": patch }),
        )
        .await;
    assert_eq!(json_of(&edited)["changed"], true);
    assert_eq!(harness.writes()[0].1, "/teams/core/notes/t1");
}

#[sqlx::test(migrations = false)]
async fn update_note_sends_typed_fields_and_rejects_empty_ones(db: PgPool) {
    let harness = Harness::new(db).await;
    harness.put_note("", "a", "body");

    let empty = harness
        .call(
            "hackmd_update_note",
            json!({ "note_id": "a", "fields": {} }),
        )
        .await;
    assert_eq!(empty["isError"], true);
    assert!(text(&empty).contains("at least one note property"));

    let fields = json!({ "title": "New", "tags": ["x"], "parentFolderId": null });
    let updated = harness
        .call(
            "hackmd_update_note",
            json!({ "note_id": "a", "fields": fields }),
        )
        .await;
    assert_eq!(json_of(&updated), json!({ "ok": true }));
    assert_eq!(harness.writes()[0].2, fields);

    let invalid = harness
        .call(
            "hackmd_update_note",
            json!({ "note_id": "a", "fields": { "readPermission": "world" } }),
        )
        .await;
    assert_eq!(invalid["isError"], true);
    assert!(text(&invalid).starts_with("Invalid arguments"));
}

#[sqlx::test(migrations = false)]
async fn read_only_sessions_cannot_write(db: PgPool) {
    let harness = Harness::new(db).await;
    harness.put_note("", "a", "title\nold\n");
    harness.patch_session(json!({ "read_only": true })).await;

    let result = harness
        .call(
            "hackmd_edit_note",
            json!({ "note_id": "a", "patch": PATCH }),
        )
        .await;
    assert_eq!(result["isError"], true);
    assert!(text(&result).contains("read-only"), "{result}");
    assert!(harness.writes().is_empty());

    let page = harness.tool_calls().await;
    let call = &page["items"][0];
    assert_eq!(call["tool"], "hackmd_edit_note");
    assert_eq!(call["status"], "denied");
    assert_eq!(call["session_name"], "Test Agent");
    assert_eq!(call["arguments"]["note_id"], "a");
}

#[sqlx::test(migrations = false)]
async fn workspace_scope_limits_teams_and_note_lookups(db: PgPool) {
    let harness = Harness::new(db).await;
    harness.put_note("core", "t1", "x");
    harness.put_note("", "p1", "y");
    harness
        .patch_session(json!({
            "scope": { "mode": "allow", "personal": true, "teams": [] }
        }))
        .await;

    let team = json!({ "kind": "team", "team_path": "core" });
    let denied = harness
        .call(
            "hackmd_get_note",
            json!({ "workspace": team, "note_id": "t1" }),
        )
        .await;
    assert_eq!(denied["isError"], true);
    assert!(text(&denied).contains("team workspace core"), "{denied}");

    // Version tools name a note alone; its workspace is looked up.
    let denied = harness
        .call("hackmd_list_versions", json!({ "note_id": "t1" }))
        .await;
    assert_eq!(denied["isError"], true);
    assert!(text(&denied).contains("team workspace core"));
    let allowed = harness
        .call("hackmd_list_versions", json!({ "note_id": "p1" }))
        .await;
    assert_eq!(json_of(&allowed)["meta"]["total"], 1);

    harness
        .patch_session(json!({
            "scope": {
                "mode": "allow", "personal": false,
                "teams": [{ "path": "core", "name": "Core" }]
            }
        }))
        .await;
    let allowed = harness
        .call(
            "hackmd_get_note",
            json!({ "workspace": team, "note_id": "t1" }),
        )
        .await;
    assert_eq!(allowed["isError"], false);
    let denied = harness.call("hackmd_list_notes", json!({})).await;
    assert!(text(&denied).contains("personal workspace"));
}

#[sqlx::test(migrations = false)]
async fn versions(db: PgPool) {
    let harness = Harness::new(db).await;
    let listed = harness
        .call(
            "hackmd_list_versions",
            json!({ "note_id": "a", "named_only": true, "limit": 5 }),
        )
        .await;
    assert_eq!(json_of(&listed)["data"][0]["id"], "v1");
    let query = harness.upstream.queries.lock().unwrap()[0].clone();
    assert_eq!(query["named_only"], "true");
    assert_eq!(query["limit"], "5");

    let created = harness
        .call(
            "hackmd_create_version",
            json!({ "note_id": "a", "name": "Draft 1" }),
        )
        .await;
    assert_eq!(json_of(&created)["id"], "v2");
    let conflict = harness
        .call(
            "hackmd_create_version",
            json!({ "note_id": "a", "name": "same" }),
        )
        .await;
    assert_eq!(conflict["isError"], true);
    assert!(text(&conflict).contains("has not changed since its latest named version"));

    let renamed = harness
        .call(
            "hackmd_update_version",
            json!({ "note_id": "a", "version_id": "v2", "name": "Final" }),
        )
        .await;
    assert_eq!(renamed["isError"], false, "{renamed}");
    let (method, _, body) = harness.writes().pop().unwrap();
    assert_eq!(method, "PATCH");
    assert_eq!(body, json!({ "base": "version:v2", "name": "Final" }));
}

#[sqlx::test(migrations = false)]
async fn rejected_tokens_ask_for_a_relink(db: PgPool) {
    let harness = Harness::unlinked(db).await;
    let result = harness.call("hackmd_get_me", json!({})).await;
    assert_eq!(result["isError"], true);
    assert!(text(&result).contains("No HackMD account is linked"));

    testing::link(&harness.state, harness.user_id, "revoked-token").await;
    let result = harness.call("hackmd_get_me", json!({})).await;
    assert_eq!(result["isError"], true);
    assert!(
        text(&result).contains("with a new HackMD API token"),
        "{result}"
    );
    let status = crate::links::status(&harness.state, harness.user_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.status, mcp_backbone::links::LinkState::NeedsRelink);

    // Re-linking clears the flag.
    testing::link(&harness.state, harness.user_id, TOKEN).await;
    let me = json_of(&harness.call("hackmd_get_me", json!({})).await);
    assert_eq!(me["userPath"], "user7");
}
