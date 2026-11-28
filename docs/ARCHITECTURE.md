# Architecture

One Rust binary serves four things on one origin (`PUBLIC_URL`):

| Path | What | Auth |
| --- | --- | --- |
| `/mcp` | MCP Streamable HTTP endpoint (rmcp) | OAuth bearer token |
| `/.well-known/*`, `/oauth/*` | OAuth 2.1 authorization server for MCP clients | public / client auth |
| `/auth/*`, `/api/*` | HackMD token sign-in and the dashboard JSON API | dashboard cookie |
| everything else | the built React dashboard (`web/dist`, SPA fallback) | — |

```
crates/hackmd   HackMD API v1 client: one `Account` per API token, an operation
                per tool, request types whose doc comments become the tool
                schemas, and the note patch engine. No HTTP serving/OAuth/MCP
                knowledge.
crates/server   the mcp-backbone service (binary `hackmd-mcp`):
  service.rs    the `Service` impl: account, error classes, workspace checks, routes
  scope.rs      `WorkspaceScope`, a session's scope
  tools.rs      the tool registry and what each request targets
  auth.rs       sign-in with a HackMD API token
  links.rs      the token in the framework's link store
  api.rs        /api/hackmd (disconnect) and /api/hackmd/teams
migrations/     legacy/ (the schema before mcp-backbone) and adopt/ (its one-time adoption)
web/            the dashboard: the package, plus the token form, account card and
                workspace picker
```

The OAuth server, dashboard sessions, tool pipeline, permission policy, call
log, outbox and the dashboard shell come from
[mcp-backbone](https://github.com/Nat1anWasTaken/mcp-backbone); identity
follows `ilearning-mcp`, where the upstream account is the user.

A database from before mcp-backbone is adopted once at startup, in one
transaction (`migrations/adopt`): users keep their IDs (the HackMD user ID
becomes `subject`, the user path `login`), `hackmd_links` becomes a link
whose secret `token` is the same sealed bytes, `workspace_scope` becomes the
session's `scope`, and the call log keeps every row. Dashboard cookies stay
valid. A test adopts a legacy database and compares it with a fresh one.

## Core concepts

- **User**: a HackMD account. Signing in means pasting a HackMD API token; the
  server checks it with `GET /me` and keys the user by HackMD's user ID. There
  is no other login.
- **Link** (the framework's `links`): the user's API token, its secret
  `token`, AES-256-GCM sealed with the user ID as associated data. Status `ok`, or `needs_relink` once HackMD
  rejected it (401); signing in again with a new token for the same account
  replaces it. Tokens are compared by SHA-256 so a stale failure never flags a
  newer token.
- **Agent session** (`agent_sessions`): one connected MCP client (claude.ai,
  ChatGPT, Claude Code, …) for one user, created when the user approves the
  OAuth consent screen. Holds the permissions: `enabled`, `read_only` (kill
  switch for every write tool), `allowed_tools` (NULL = all), `use_outbox`,
  and `scope` (which workspaces it may touch: all, or the personal workspace
  and/or listed teams). Re-authorizing the same client reuses the
  active session.
- **Tool call** (`tool_calls`): every MCP tool invocation, including denied
  ones, with bounded and redacted arguments and results.

Permissions are re-read from the database on every tool call, so dashboard
changes apply immediately to live connections.

## Tools

Sixteen tools, all prefixed `hackmd_`. The first ten keep the names and
parameters of the original hackmd-mcp-proxy, and a test pins them.

| Tool | Kind | HackMD API |
| --- | --- | --- |
| `list_notes` | read | `GET [teams/{t}/]notes`, then filtered, sorted and paged here |
| `get_note` | read | `GET [teams/{t}/]notes/{id}`, plus the note's `patch_path` |
| `create_note` | write | `POST [teams/{t}/]notes` |
| `edit_note` | write | read the note, apply a Codex-style patch, `PATCH` only if it changed |
| `update_note` | write | `PATCH [teams/{t}/]notes/{id}` with metadata fields |
| `delete_note` | write (destructive) | `DELETE [teams/{t}/]notes/{id}` |
| `list_folders` / `create_folder` / `update_folder` / `delete_folder` | read / write | `[teams/{t}/]folders[/{id}]` |
| `get_me` | read | `GET /me`, including the user's teams |
| `list_versions` / `get_version` / `compare_versions` | read | `GET notes/{id}/versions[/{v} or /compare]` |
| `create_version` / `update_version` | write | `POST` / `PATCH notes/{id}/versions` |

Note and folder tools take an optional `workspace`, `{"kind": "personal"}` (the
default) or `{"kind": "team", "team_path": "..."}`. Version tools name a note
by ID alone.

`edit_note` patches use the `*** Begin Patch` / `*** Update File: <patch_path>`
format. Hunks match their context exactly once, after an optional `@@ <line>`
anchor, retrying with whitespace ignored. Add-only hunks append or insert after
the anchor, and `*** End of File` pins a hunk to the end. Before writing, the
note is re-read with `If-None-Match`, and the edit fails rather than overwrite a
concurrent change.

## Tool calls

The framework runs every `tools/call` through one path:

1. Load the session's permissions fresh from Postgres.
2. Parse the arguments into the `hackmd` request type. The same type provides
   the tool's input schema.
3. Admit the call with `policy::check_tool`: the session must be enabled, the
   tool must be in `allowed_tools`, and write tools are refused on read-only
   sessions. `tools/list` shows exactly the tools that pass this check.
4. Unseal the user's HackMD token. A missing token or one flagged
   `needs_relink` ends the call with a message telling the agent to have the
   user sign in again.
5. Check the request's workspace against the session's `scope`. For version tools,
   which name a note alone, a restricted scope looks up the note's `teamPath`
   first.
6. Log the call as `running` and execute it in a detached task, so a client
   disconnect can't leave it half done or unlogged. A HackMD 401 flags the
   token `needs_relink`; a success bumps `last_ok_at` (at most once a minute).

Failures are tool results with `isError: true`, written for the agent. Only an
unknown tool, a revoked session or an internal error is a JSON-RPC error.

## Conventions

- SQL: runtime-checked `sqlx::query` / `query_as::<_, T>` with `#[derive(FromRow)]`
  (no compile-time macros, so builds need no database).
- Secrets (cookies, codes, tokens) are random 256-bit strings; only SHA-256
  hashes are stored (`crypto::random_token` / `crypto::hash_token`).
- HackMD tokens are AES-256-GCM sealed with `ENCRYPTION_KEY`; they are never
  logged, and `hackmd::Error` messages never contain them or raw HackMD bodies.
- Dashboard API errors: `{"error": "<code>", "message": "..."}` via `AppError`.
  OAuth errors use RFC 6749 `{"error", "error_description"}`.
- Mutating `/api` and `/auth` requests must be `application/json` and, when an
  `Origin` header is present, it must equal `PUBLIC_URL` (CSRF defence
  alongside `SameSite=Lax` cookies).
- Times are RFC 3339 strings; IDs are UUID strings.

## Dashboard API contract

### Shapes

```ts
type User = {
  id: string
  hackmd_user_id: string
  user_path: string | null
  name: string
  email: string | null
  avatar_url: string | null
}

type LinkStatus = {
  status: "ok" | "needs_relink"
  last_ok_at: string | null
  last_error: string | null
  updated_at: string
}

type Me = { user: User; hackmd: LinkStatus | null }

type Team = { path: string; name: string; logo: string | null }

type WorkspaceScope = {
  mode: "all" | "allow"
  personal: boolean // with "allow"
  teams: { path: string; name: string }[] // with "allow"
}

type AgentSession = {
  id: string
  name: string
  client_id: string
  client_name: string | null
  origin: string | null
  enabled: boolean
  read_only: boolean
  allowed_tools: string[] | null // null = all tools
  scope: WorkspaceScope // `{}` for a new session: every workspace
  created_at: string
  last_used_at: string | null
}

type ToolInfo = { name: string; title: string; description: string; kind: "read" | "write" }

type ToolCall = {
  id: string
  session_id: string
  session_name: string
  tool: string
  arguments: Record<string, unknown> // long strings truncated
  status: "running" | "ok" | "error" | "denied"
  result: unknown | null
  error: string | null
  duration_ms: number | null
  created_at: string
}

type OAuthRequest = {
  client_id: string
  client_name: string | null
  client_uri: string | null
  logo_uri: string | null
  origin: string | null
  redirect_uri: string
  scope: string
  existing_session: { name: string; read_only: boolean } | null
}
```

### Endpoints

| Method | Path | Body / query | Response |
| --- | --- | --- | --- |
| POST | `/auth/hackmd` | `{token, next?}` | `{redirect_to}`; sets the session cookie when signing in. `400 invalid_token`, `403` (allowlist), `409` (token of another account while signed in) |
| POST | `/auth/logout` | — | `204` |
| GET | `/api/me` | — | `Me` |
| DELETE | `/api/hackmd` | — | `204`; deletes the user and everything they own here |
| GET | `/api/hackmd/teams` | — | `Team[]`, live from HackMD |
| GET | `/api/tools` | — | `ToolInfo[]` |
| GET | `/api/sessions` | — | `AgentSession[]` (active only) |
| GET | `/api/sessions/:id` | — | `AgentSession` |
| PATCH | `/api/sessions/:id` | partial `{name, enabled, read_only, allowed_tools, scope, use_outbox, outbox_ttl_minutes}` | `AgentSession` |
| DELETE | `/api/sessions/:id` | — | `204`; revokes its tokens, keeps its log |
| GET | `/api/tool-calls` | `session_id?, status?, before?, limit?` | `{items: ToolCall[], next_before}` |
| GET | `/api/tool-calls/:id` | — | `ToolCall` |
| GET | `/api/oauth/requests/:id` | — | `OAuthRequest` |
| POST | `/api/oauth/requests/:id` | `{approve, name?, read_only?}` | `{redirect_to}` |
| GET | `/healthz` | — | `ok` |

### OAuth flow (MCP client side)

1. Client hits `/mcp` → `401` with `WWW-Authenticate: Bearer resource_metadata="<PUBLIC_URL>/.well-known/oauth-protected-resource/mcp"`.
2. Client reads protected-resource and authorization-server metadata, registers
   via `POST /oauth/register` (or uses a CIMD URL as `client_id`). Metadata
   document clients are public (`none`) or, like ChatGPT, authenticate with
   `private_key_jwt`: an RS256/PS256/ES256 assertion signed with a key from a
   `jwks_uri` on the client's own host (RFC 7523).
3. Browser goes to `GET /oauth/authorize?...` (PKCE S256 required). The server
   stores an `oauth_requests` row and redirects to `/consent/<id>`; if not
   signed in, first to `/login?next=<authorize URL>`.
4. The SPA consent page shows `GET /api/oauth/requests/:id`; on approve it
   `POST`s and navigates to `redirect_to` (client redirect URI with `code`).
5. Client exchanges the code at `POST /oauth/token`, then refreshes with
   rotating refresh tokens; reusing a refresh token revokes its family.

### SPA routes

`/login` (paste a HackMD token), `/` (overview: account, token, MCP URL),
`/sessions`, `/sessions/:id` (permissions, tools, workspaces), `/activity`
(tool-call log), `/consent/:id`, `/connect` (instructions for adding the MCP
URL to each AI client).
