# hackmd-mcp

A remote [MCP](https://modelcontextprotocol.io) server that lets AI agents
(claude.ai, ChatGPT, Gemini, Claude Code, …) read and edit **your HackMD notes**,
under permissions you control from a web dashboard. It replaces
hackmd-mcp-proxy, keeping its tools.

- **Sign in** to the dashboard with a HackMD API token. Your HackMD account is
  your identity here; the token is stored encrypted.
- **Connect** an AI client by adding `https://<host>/mcp` as a custom connector.
  It authenticates with OAuth 2.1 (dynamic client registration or client ID
  metadata documents, PKCE), and you approve it on a consent screen.
- **Control** every connected agent separately:
  - enable or disable it;
  - turn on the read-only kill switch;
  - choose the allowed tools;
  - limit it to your personal workspace and/or chosen teams.
- **Audit** every tool call in the activity log.

Tools: notes (list, get, create, patch-based edit, update, delete), folders,
your account and teams, and note versions (list, get, compare, name, rename).
See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the design, the tool list
and the dashboard API contract.

## Development

Requirements: Rust (the pinned toolchain in `rust-toolchain.toml` installs
itself), Node 22 with pnpm, Docker.

```sh
docker compose -f docker-compose.dev.yml up -d   # Postgres on localhost:5444
cp .env.example .env                             # then fill it in, see below
cargo run -p hackmd-mcp                          # API + MCP on :8080, migrations run at startup
cd web && pnpm install && pnpm dev               # dashboard on :5173, proxies to :8080
```

Set `PUBLIC_URL=http://localhost:5173` while using the Vite dev server, so that
OAuth redirects and cookies stay on one origin.

Tests (the integration tests use the dev database from `DATABASE_URL` and a
fake HackMD API; none reach hackmd.io):

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd web && pnpm lint && pnpm build
```

### Configuration

| Variable | Purpose |
| --- | --- |
| `PUBLIC_URL` | External origin. OAuth metadata and the MCP resource URL derive from it. |
| `DATABASE_URL` | Postgres connection string. |
| `ENCRYPTION_KEY` | 32 bytes, base64 (`openssl rand -base64 32`). It encrypts HackMD tokens at rest. |
| `ALLOWED_HACKMD_USERS` | Comma-separated HackMD user paths or emails allowed to sign in. Leave it empty to allow anyone. |
| `HACKMD_API_URL` | HackMD API root (default `https://api.hackmd.io/v1`). |
| `BIND_ADDR`, `WEB_DIST` | Listen address (default `0.0.0.0:8080`) and dashboard build directory (default `web/dist`). |

## Deployment

The production stack is `docker-compose.yml`: the app image (the multi-stage
`Dockerfile` builds both the dashboard and the server) plus Postgres 17. It is
deployed as a Dokploy compose service built from this repository.

1. Put the variables above, plus `POSTGRES_PASSWORD`, in the service's
   environment. `DATABASE_URL` is derived from `POSTGRES_PASSWORD` in the
   compose file.
2. Point a domain at service `app`, port `8080`. TLS terminates at the
   Cloudflare tunnel in front of Traefik.
3. In claude.ai, open Settings, then Connectors, then Add custom connector.
   Enter `https://<host>/mcp` and leave the OAuth fields empty.
