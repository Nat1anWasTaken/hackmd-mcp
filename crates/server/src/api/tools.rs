use axum::Json;

use crate::{auth::CurrentUser, mcp};

pub async fn list(_: CurrentUser) -> Json<Vec<&'static mcp::Tool>> {
    Json(mcp::configurable_tools().collect())
}

#[cfg(test)]
mod tests {
    use axum::http::{Method, StatusCode};
    use serde_json::json;
    use sqlx::PgPool;

    use crate::testing::{self, dashboard_request, send};

    #[sqlx::test(migrations = "../../migrations")]
    async fn lists_tools_and_validates_session_tools(db: PgPool) {
        let state = testing::state(db);
        let (user_id, cookie) = testing::sign_in(&state, 7).await;
        let session = testing::agent_session(&state, user_id).await;
        let app = crate::app(state);

        let tools = send(
            &app,
            dashboard_request(Method::GET, "/api/tools", &cookie, None),
        )
        .await
        .json();
        assert_eq!(tools.as_array().unwrap().len(), 16);
        assert_eq!(
            tools[0],
            json!({
                "name": "hackmd_list_notes",
                "title": "List notes",
                "description": tools[0]["description"],
                "kind": "read",
            })
        );

        let uri = format!("/api/sessions/{session}");
        let patch = json!({ "allowed_tools": ["hackmd_get_me", "check_outbox"] });
        let request = dashboard_request(Method::PATCH, &uri, &cookie, Some(&patch));
        let response = send(&app, request).await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST);
        assert!(
            response.json()["message"]
                .as_str()
                .unwrap()
                .contains("check_outbox")
        );
    }
}
