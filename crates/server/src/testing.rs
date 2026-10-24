//! Fixtures for tests that drive the HTTP app against a `#[sqlx::test]` database.

use std::sync::Arc;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode, header},
};
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;

use crate::{config::Config, state::AppState};

pub const PUBLIC_URL: &str = "https://mcp.example.test";

/// App state whose HackMD API is unreachable; tests that need one replace
/// `hackmd` with a client for [`serve`]d stand-in.
pub fn state(db: PgPool) -> AppState {
    let config = Arc::new(Config {
        bind_addr: ([127, 0, 0, 1], 0).into(),
        public_url: PUBLIC_URL.to_owned(),
        database_url: String::new(),
        hackmd_api_url: "http://127.0.0.1:9".to_owned(),
        allowed_hackmd_users: Vec::new(),
        encryption_key: [0; 32],
        web_dist: "/nonexistent".into(),
    });
    AppState {
        hackmd: hackmd::HackMd::new(&config.hackmd_api_url),
        db,
        config,
        client_assertions: Default::default(),
    }
}

/// Serves `app` on a loopback port; returns its base URL.
pub async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let addr = listener.local_addr().expect("local address");
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{addr}")
}

pub struct Response {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).expect("JSON response body")
    }

    pub fn location(&self) -> &str {
        self.headers[header::LOCATION]
            .to_str()
            .expect("ASCII Location")
    }
}

pub async fn send(app: &Router, request: Request<Body>) -> Response {
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("infallible router");
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("readable body")
        .to_vec();
    Response {
        status,
        headers,
        body,
    }
}

/// A signed-in dashboard user; returns its ID and the `Cookie` header value.
pub async fn sign_in(state: &AppState, n: i64) -> (uuid::Uuid, String) {
    let user_id = sqlx::query_scalar(
        "INSERT INTO users (hackmd_user_id, user_path, name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(format!("hm-{n}"))
    .bind(format!("user{n}"))
    .bind(format!("User {n}"))
    .fetch_one(&state.db)
    .await
    .expect("insert user");
    let cookie = crate::auth::start_session(&state.db, &state.config, user_id)
        .await
        .expect("start session");
    (user_id, format!("{}={}", cookie.name(), cookie.value()))
}

/// Links `token` as the user's HackMD API token.
pub async fn link(state: &AppState, user_id: uuid::Uuid, token: &str) {
    crate::links::store(&state.db, &state.config, user_id, token)
        .await
        .expect("store HackMD token");
}

/// An agent session for `user_id`, with a freshly inserted public client.
pub async fn agent_session(state: &AppState, user_id: uuid::Uuid) -> uuid::Uuid {
    let client_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO oauth_clients (client_id, client_name, redirect_uris, grant_types, source)
         VALUES ($1, 'Test Agent', ARRAY['https://agent.example/cb'], ARRAY['authorization_code'], 'dcr')",
    )
    .bind(&client_id)
    .execute(&state.db)
    .await
    .expect("insert client");
    sqlx::query_scalar(
        "INSERT INTO agent_sessions (user_id, client_id, name) VALUES ($1, $2, 'Test Agent')
         RETURNING id",
    )
    .bind(user_id)
    .bind(&client_id)
    .fetch_one(&state.db)
    .await
    .expect("insert agent session")
}

/// A JSON request from the dashboard, as the browser would send it.
pub fn dashboard_request(
    method: axum::http::Method,
    uri: &str,
    cookie: &str,
    body: Option<&Value>,
) -> Request<Body> {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, PUBLIC_URL);
    match body {
        Some(body) => request
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    }
    .expect("valid request")
}
