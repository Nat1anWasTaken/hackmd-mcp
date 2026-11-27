//! Signing in to the dashboard with a HackMD API token, which also links it.

use axum::{Json, extract::State, http::StatusCode};
use axum_extra::extract::CookieJar;
use mcp_backbone::{
    AppError, AppResult, AppState,
    auth::{self, CurrentUser, Profile},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{links, service::HackMdService};

/// HackMD tokens are short; anything much longer is not one.
const MAX_TOKEN_LEN: usize = 512;

#[derive(Deserialize)]
pub struct SignIn {
    token: String,
    next: Option<String>,
}

/// `POST /auth/hackmd`: checks the token with HackMD, signs its account in
/// (or, when already signed in as it, re-links it) and stores the token.
pub async fn sign_in(
    State(state): State<AppState<HackMdService>>,
    current: Option<CurrentUser>,
    jar: CookieJar,
    Json(body): Json<SignIn>,
) -> AppResult<(CookieJar, Json<Value>)> {
    let token = body.token.trim();
    if token.is_empty() || token.len() > MAX_TOKEN_LEN || token.contains(char::is_whitespace) {
        return Err(AppError::BadRequest("Paste a HackMD API token.".into()));
    }
    let next = auth::local_path_or_root(body.next.as_deref());
    let me = match state.service.hackmd.account(token).me().await {
        Ok(me) => me,
        Err(hackmd::Error::InvalidToken) => {
            return Err(AppError::coded(
                StatusCode::BAD_REQUEST,
                "invalid_token",
                "HackMD rejected this API token. Check that it was copied in full and has not \
                 been deleted.",
            ));
        }
        Err(err) => return Err(AppError::Upstream(err.to_string())),
    };
    let mut profile = Profile::new(me.id.clone());
    profile.login = me.user_path.clone();
    profile.name = Some(me.name.clone());
    profile.email = me.email.clone();
    profile.avatar_url = me.photo.clone();
    if !auth::is_allowed(&state.config, &profile) {
        tracing::info!(user_path = ?me.user_path, "HackMD user not on the allowlist");
        return Err(AppError::Forbidden(
            "This HackMD account is not allowed to use this server.".into(),
        ));
    }
    if let Some(current) = &current
        && current.subject != me.id
    {
        return Err(AppError::Conflict(format!(
            "That token belongs to the HackMD account {}, not the one you are signed in as. \
             Sign out first to switch accounts.",
            me.user_path.as_deref().unwrap_or(&me.name)
        )));
    }
    let user_id = auth::upsert_user(&state.db, &profile).await?;
    links::store(&state, user_id, token).await?;
    let jar = match current {
        Some(_) => jar,
        None => jar.add(auth::start_session(&state, user_id).await?),
    };
    Ok((jar, Json(json!({ "redirect_to": next }))))
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

    #[sqlx::test(migrations = false)]
    async fn signs_in_relinks_and_refuses_other_accounts(db: PgPool) {
        let state = testing::state_with_hackmd(db.clone(), &fake_hackmd().await).await;
        let app = mcp_backbone::router(state);

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
        assert_eq!(me.json()["user"]["login"], "ada");
        assert_eq!(me.json()["user"]["subject"], "hm-1");
        assert_eq!(me.json()["link"]["status"], "ok");

        // The stored token is sealed, never plain.
        let sealed: Vec<u8> = sqlx::query_scalar("SELECT sealed FROM link_secrets")
            .fetch_one(&db)
            .await
            .unwrap();
        assert!(!sealed.windows(10).any(|w| w == b"good-token"));

        // Re-linking the same account while signed in keeps the session.
        sqlx::query("UPDATE links SET status = 'needs_relink'")
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
        assert_eq!(me.json()["link"]["status"], "ok");

        let other = send(&app, sign_in_request("other-token", &cookie)).await;
        assert_eq!(other.status, StatusCode::CONFLICT);
        let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(users, 1, "the refused sign-in was rolled back");
    }

    #[sqlx::test(migrations = false)]
    async fn allowlist_applies(db: PgPool) {
        let mut state = testing::state_with_hackmd(db, &fake_hackmd().await).await;
        let mut config = (*state.config).clone();
        config.allowed_users = vec!["ada@example.test".into()];
        state.core.config = config.into();
        let app = mcp_backbone::router(state);

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
