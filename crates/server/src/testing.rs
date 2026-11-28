//! Test state for HackMD on a migrated test database. The framework's
//! `testing` module has the rest.

use mcp_backbone::{App, AppState, testing};
use sqlx::PgPool;
use uuid::Uuid;

pub use mcp_backbone::testing::{
    PUBLIC_URL, Response, access_token, agent_session, dashboard_request, send, serve, sign_in,
};

use crate::{links, service::HackMdService};

/// App state whose HackMD API is at `api_url` (default: a closed port).
pub async fn state_with_hackmd(db: PgPool, api_url: &str) -> AppState<HackMdService> {
    testing::migrate(&db, None).await;
    let service = HackMdService {
        hackmd: hackmd::HackMd::new(api_url),
    };
    testing::app_state(App::new(service), db).await
}

pub async fn state(db: PgPool) -> AppState<HackMdService> {
    state_with_hackmd(db, "http://127.0.0.1:9").await
}

/// Links `token` as the user's HackMD API token.
pub async fn link(state: &AppState<HackMdService>, user_id: Uuid, token: &str) {
    links::store(state, user_id, token)
        .await
        .expect("store HackMD token");
}
