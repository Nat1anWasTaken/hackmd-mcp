//! Sign-in by HackMD API token: the HackMD account is the dashboard user.
//! Signing in again with a new token for the same account replaces the
//! stored token, which is how a revoked token is re-linked.

use axum::{Json, extract::State};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{CurrentUser, session};
use crate::{
    error::{AppError, AppResult},
    links,
    state::AppState,
};

/// HackMD tokens are short; anything much longer is not one.
const MAX_TOKEN_LEN: usize = 512;

#[derive(Deserialize)]
pub struct SignIn {
    token: String,
    next: Option<String>,
}

pub async fn sign_in(
    State(state): State<AppState>,
    current: Option<CurrentUser>,
    jar: CookieJar,
    Json(body): Json<SignIn>,
) -> AppResult<(CookieJar, Json<Value>)> {
    let token = body.token.trim();
    if token.is_empty() || token.len() > MAX_TOKEN_LEN || token.contains(char::is_whitespace) {
        return Err(AppError::BadRequest("Paste a HackMD API token.".into()));
    }
    let next = body
        .next
        .filter(|next| is_local_path(next))
        .unwrap_or_else(|| "/".into());

    let me = state.hackmd.account(token).me().await?;
    if !state
        .config
        .is_hackmd_user_allowed(me.user_path.as_deref(), me.email.as_deref())
    {
        tracing::info!(user_path = ?me.user_path, "HackMD user not on the allowlist");
        return Err(AppError::Forbidden(
            "This HackMD account is not allowed to use this server.".into(),
        ));
    }

    let mut tx = state.db.begin().await?;
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (hackmd_user_id, user_path, name, email, avatar_url)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (hackmd_user_id) DO UPDATE SET user_path = EXCLUDED.user_path,
             name = EXCLUDED.name, email = EXCLUDED.email, avatar_url = EXCLUDED.avatar_url,
             last_login_at = now()
         RETURNING id",
    )
    .bind(&me.id)
    .bind(&me.user_path)
    .bind(&me.name)
    .bind(&me.email)
    .bind(&me.photo)
    .fetch_one(&mut *tx)
    .await?;
    if let Some(current) = &current
        && current.id != user_id
    {
        return Err(AppError::Conflict(format!(
            "That token belongs to the HackMD account {}, not the one you are signed in as. \
             Sign out first to switch accounts.",
            me.user_path.as_deref().unwrap_or(&me.name)
        )));
    }
    links::store(&mut *tx, &state.config, user_id, token).await?;
    tx.commit().await?;

    let jar = match current {
        Some(_) => jar,
        None => jar.add(session::start(&state.db, &state.config, user_id).await?),
    };
    Ok((jar, Json(json!({ "redirect_to": next }))))
}

/// Whether `next` is a path on this origin, so redirecting to it after sign-in
/// cannot leave the site. Browsers treat `\` like `/` and drop tabs and
/// newlines, so `/\host` and `/\t/host` would be protocol-relative URLs.
pub fn is_local_path(next: &str) -> bool {
    next.starts_with('/')
        && !next.starts_with("//")
        && !next.starts_with("/\\")
        && !next.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        http::{Method, StatusCode},
        routing::get,
    };
    use serde_json::json;
    use sqlx::PgPool;

    use super::*;
    use crate::testing::{self, dashboard_request, send};

    #[test]
    fn accepts_only_local_paths() {
        for ok in [
            "/",
            "/sessions/1",
            "/oauth/authorize?client_id=x&redirect_uri=https%3A%2F%2Fa",
        ] {
            assert!(is_local_path(ok), "{ok}");
        }
        for bad in [
            "",
            "sessions",
            "https://evil.example",
            "//evil.example",
            "/\\evil.example",
            "/\t/evil.example",
            "/\n/evil.example",
            "javascript:alert(1)",
        ] {
            assert!(!is_local_path(bad), "{bad:?}");
        }
    }

    /// A HackMD stand-in whose `/me` answers for `good-token` and
    /// `other-token` only.
    async fn fake_hackmd() -> String {
        let app = Router::new().route(
            "/me",
            get(|headers: axum::http::HeaderMap| async move {
                let auth = headers["authorization"].to_str().unwrap().to_owned();
                match auth.as_str() {
                    "Bearer good-token" => Ok(Json(json!({
                        "id": "hm-1", "name": "Ada", "userPath": "ada",
                        "email": "ada@example.test", "photo": null, "teams": []
                    }))),
                    "Bearer other-token" => Ok(Json(json!({
                        "id": "hm-2", "name": "Bob", "userPath": "bob", "teams": []
                    }))),
                    _ => Err(StatusCode::UNAUTHORIZED),
                }
            }),
        );
        testing::serve(app).await
    }

    fn sign_in_request(token: &str, cookie: &str) -> axum::http::Request<axum::body::Body> {
        dashboard_request(
            Method::POST,
            "/auth/hackmd",
            cookie,
            Some(&json!({ "token": token, "next": "//evil.example" })),
        )
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn signs_in_relinks_and_refuses_other_accounts(db: PgPool) {
        let mut state = testing::state(db.clone());
        state.hackmd = hackmd::HackMd::new(&fake_hackmd().await);
        let app = crate::app(state);

        let bad = send(&app, sign_in_request("wrong", "")).await;
        assert_eq!(bad.status, StatusCode::BAD_REQUEST);
        assert_eq!(bad.json()["error"], "invalid_token");

        let ok = send(&app, sign_in_request("good-token", "")).await;
        assert_eq!(ok.status, StatusCode::OK);
        // A forged `next` falls back to the dashboard root.
        assert_eq!(ok.json()["redirect_to"], "/");
        let cookie = ok.headers["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let me = send(
            &app,
            dashboard_request(Method::GET, "/api/me", &cookie, None),
        )
        .await;
        assert_eq!(me.json()["user"]["user_path"], "ada");
        assert_eq!(me.json()["hackmd"]["status"], "ok");

        // The stored token is sealed, never plain.
        let sealed: Vec<u8> = sqlx::query_scalar("SELECT token_sealed FROM hackmd_links")
            .fetch_one(&db)
            .await
            .unwrap();
        assert!(!sealed.windows(10).any(|w| w == b"good-token"));

        // Re-linking the same account while signed in keeps the session.
        sqlx::query("UPDATE hackmd_links SET status = 'needs_relink'")
            .execute(&db)
            .await
            .unwrap();
        let relinked = send(&app, sign_in_request("good-token", &cookie)).await;
        assert_eq!(relinked.status, StatusCode::OK);
        assert!(relinked.headers.get("set-cookie").is_none());
        let me = send(
            &app,
            dashboard_request(Method::GET, "/api/me", &cookie, None),
        )
        .await;
        assert_eq!(me.json()["hackmd"]["status"], "ok");

        let other = send(&app, sign_in_request("other-token", &cookie)).await;
        assert_eq!(other.status, StatusCode::CONFLICT);
        let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(users, 1, "the refused sign-in was rolled back");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn allowlist_applies(db: PgPool) {
        let mut state = testing::state(db);
        state.hackmd = hackmd::HackMd::new(&fake_hackmd().await);
        let mut config = (*state.config).clone();
        config.allowed_hackmd_users = vec!["ada@example.test".into()];
        state.config = config.into();
        let app = crate::app(state);

        assert_eq!(
            send(&app, sign_in_request("other-token", "")).await.status,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            send(&app, sign_in_request("good-token", "")).await.status,
            StatusCode::OK
        );
    }
}
