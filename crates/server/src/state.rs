use std::sync::Arc;

use sqlx::PgPool;

use crate::{config::Config, oauth::ClientAssertions};

/// Shared application state. Every field is cheap to clone.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: PgPool,
    pub hackmd: hackmd::HackMd,
    pub client_assertions: Arc<ClientAssertions>,
}
