//! HackMD API v1 for agents: one [`Account`] per user's API token, with an
//! operation per MCP tool and request types whose doc comments become the tool
//! schemas. Knows nothing about HTTP serving, OAuth or MCP.

mod error;
mod ops;
pub mod patch;
mod types;
mod workspace;

use std::{fmt, sync::Arc, time::Duration};

use reqwest::{Method, RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

pub use error::{Error, Result};
pub use ops::me::{Me, Team};
pub use types::*;
pub use workspace::Workspace;

/// The HackMD API, shared by every account.
#[derive(Clone)]
pub struct HackMd {
    http: reqwest::Client,
    base_url: Arc<str>,
}

impl HackMd {
    /// `base_url` is the API root, such as `https://api.hackmd.io/v1`.
    pub fn new(base_url: &str) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("hackmd-mcp/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .expect("the TLS backend is compiled in");
        Self {
            http,
            base_url: base_url.trim_end_matches('/').into(),
        }
    }

    /// A handle acting as the owner of `token`.
    pub fn account(&self, token: impl Into<Arc<str>>) -> Account {
        Account {
            api: self.clone(),
            token: token.into(),
        }
    }
}

/// A HackMD user, by their API token. Cheap to clone.
#[derive(Clone)]
pub struct Account {
    api: HackMd,
    token: Arc<str>,
}

impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Account").finish_non_exhaustive()
    }
}

impl Account {
    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.api
            .http
            .request(method, format!("{}/{path}", self.api.base_url))
            .bearer_auth(&*self.token)
    }

    /// Sends `request` and decodes its JSON body; an empty success body
    /// becomes `{"ok": true}`.
    async fn send(&self, request: RequestBuilder) -> Result<Value> {
        let response = request.send().await.map_err(Error::network)?;
        self.send_response(response).await
    }

    async fn send_response(&self, response: reqwest::Response) -> Result<Value> {
        let status = response.status();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok()?.parse().ok());
        let bytes = response.bytes().await.map_err(Error::network)?;
        if status.is_success() {
            if bytes.iter().all(u8::is_ascii_whitespace) {
                return Ok(json!({ "ok": true }));
            }
            return serde_json::from_slice(&bytes)
                .map_err(|err| Error::BadResponse(err.to_string()));
        }
        Err(Error::from_status(status, &bytes, retry_after))
    }

    async fn get(&self, path: &str) -> Result<Value> {
        self.send(self.request(Method::GET, path)).await
    }

    async fn get_as<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        serde_json::from_value(self.get(path).await?)
            .map_err(|err| Error::BadResponse(err.to_string()))
    }

    async fn post(&self, path: &str, body: &impl serde::Serialize) -> Result<Value> {
        self.send(self.request(Method::POST, path).json(body)).await
    }

    async fn patch(&self, path: &str, body: &impl serde::Serialize) -> Result<Value> {
        self.send(self.request(Method::PATCH, path).json(body))
            .await
    }

    async fn delete(&self, path: &str) -> Result<Value> {
        self.send(self.request(Method::DELETE, path)).await
    }

    /// `GET path`, or `None` when it still has the entity tag `etag`.
    async fn get_if_changed(&self, path: &str, etag: Option<&str>) -> Result<Option<Value>> {
        let mut request = self.request(Method::GET, path);
        if let Some(etag) = etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        let response = request.send().await.map_err(Error::network)?;
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(None);
        }
        self.send_response(response).await.map(Some)
    }
}
