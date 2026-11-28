mod api;
mod auth;
mod config;
mod links;
mod scope;
mod service;
#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;
mod tools;

use mcp_backbone::migrate::Adoption;

use crate::service::HackMdService;

/// Brings a database from before mcp-backbone into the framework's schema.
pub static ADOPTION: Adoption = Adoption {
    legacy_versions: &[1],
    before: include_str!("../../../migrations/adopt/before.sql"),
    after: include_str!("../../../migrations/adopt/after.sql"),
    service_baseline: None,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let service = HackMdService {
        hackmd: hackmd::HackMd::new(&config::api_url()),
    };
    mcp_backbone::App::new(service)
        .outbox(true)
        .push(true)
        .adopt(&ADOPTION)
        .serve()
        .await
}

#[cfg(test)]
mod adoption_tests {
    use mcp_backbone::{
        crypto::sealed,
        migrate,
        sqlx::{self, PgPool, migrate::Migrator},
        testing::{TempDatabase, schema},
    };
    use serde_json::json;
    use uuid::Uuid;

    use super::ADOPTION;
    use crate::{scope::WorkspaceScope, testing};

    static LEGACY: Migrator = sqlx::migrate!("../../migrations/legacy");

    /// A database from before mcp-backbone, with a linked user, a scoped
    /// session and a logged call, is adopted with every row, matches a fresh
    /// schema, and its token still opens.
    #[sqlx::test(migrations = false)]
    async fn legacy_databases_are_adopted(db: PgPool) {
        LEGACY.run(&db).await.unwrap();
        let user_id = Uuid::new_v4();
        let token = sealed::seal(&[7; 32], user_id.as_bytes(), b"legacy-token").unwrap();
        sqlx::query(
            "INSERT INTO users (id, hackmd_user_id, user_path, name, email)
             VALUES ($1, 'hm-9', 'ada', 'Ada', 'ada@example.test')",
        )
        .bind(user_id)
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO hackmd_links (user_id, token_sealed, token_hash, last_ok_at)
             VALUES ($1, $2, $3, now())",
        )
        .bind(user_id)
        .bind(token)
        .bind(mcp_backbone::links::token_hash("legacy-token"))
        .execute(&db)
        .await
        .unwrap();
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "INSERT INTO oauth_clients (client_id, redirect_uris, grant_types, source)
             VALUES ('client', ARRAY['https://agent.example/cb'], ARRAY['authorization_code'], 'dcr');
             INSERT INTO agent_sessions (id, user_id, client_id, name, workspace_scope)
             VALUES ('00000000-0000-0000-0000-0000000000a1', '{user_id}', 'client', 'Claude',
                     '{{\"mode\":\"allow\",\"personal\":false,\"teams\":[{{\"path\":\"core\",\"name\":\"Core\"}}]}}');
             INSERT INTO tool_calls (user_id, agent_session_id, tool, arguments, status)
             VALUES ('{user_id}', '00000000-0000-0000-0000-0000000000a1', 'hackmd_get_me', '{{}}', 'ok');"
        )))
        .execute(&db)
        .await
        .unwrap();

        migrate::run(&db, None, Some(&ADOPTION)).await.unwrap();
        migrate::run(&db, None, Some(&ADOPTION)).await.unwrap();

        let fresh = TempDatabase::new(&db).await;
        migrate::run(&fresh.pool, None, None).await.unwrap();
        assert_eq!(schema(&db).await, schema(&fresh.pool).await);
        fresh.drop().await;

        let (subject, login, email): (String, String, String) =
            sqlx::query_as("SELECT subject, login, email FROM users")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(
            (subject.as_str(), login.as_str(), email.as_str()),
            ("hm-9", "ada", "ada@example.test")
        );
        let scope: serde_json::Value = sqlx::query_scalar("SELECT scope FROM agent_sessions")
            .fetch_one(&db)
            .await
            .unwrap();
        let scope: WorkspaceScope = serde_json::from_value(scope).unwrap();
        assert!(!scope.allows(None) && scope.allows(Some("core")));
        let calls: i64 = sqlx::query_scalar("SELECT count(*) FROM tool_calls")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(calls, 1);

        let state = testing::state(db.clone()).await;
        let record = mcp_backbone::links::LinkStore::new(&state)
            .load(user_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            record.secret(&state, crate::links::TOKEN).unwrap(),
            "legacy-token"
        );
        let status = crate::links::status(&state, user_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.status, mcp_backbone::links::LinkState::Ok);
        let _ = json!({});
    }
}
