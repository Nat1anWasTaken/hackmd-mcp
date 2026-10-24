use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use url::Url;

/// Runtime configuration, read once from the environment at startup.
#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: SocketAddr,
    /// Externally visible origin, without a trailing slash (e.g. `https://hackmd-mcp.nath.tw`).
    pub public_url: String,
    pub database_url: String,
    /// HackMD API root, without a trailing slash.
    pub hackmd_api_url: String,
    /// HackMD user paths or emails (lowercase) allowed to sign in. Empty means anyone.
    pub allowed_hackmd_users: Vec<String>,
    /// Key for encrypting HackMD API tokens at rest.
    pub encryption_key: [u8; 32],
    /// Directory containing the built dashboard SPA.
    pub web_dist: PathBuf,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let public_url = required("PUBLIC_URL")?.trim_end_matches('/').to_owned();
        Url::parse(&public_url).context("PUBLIC_URL is not a valid URL")?;

        let encryption_key: [u8; 32] = STANDARD
            .decode(required("ENCRYPTION_KEY")?)
            .context("ENCRYPTION_KEY must be base64")?
            .try_into()
            .ok()
            .context("ENCRYPTION_KEY must decode to exactly 32 bytes")?;

        Ok(Self {
            bind_addr: optional("BIND_ADDR")
                .unwrap_or_else(|| "0.0.0.0:8080".into())
                .parse()
                .context("BIND_ADDR is not a socket address")?,
            public_url,
            database_url: required("DATABASE_URL")?,
            hackmd_api_url: optional("HACKMD_API_URL")
                .unwrap_or_else(|| "https://api.hackmd.io/v1".into())
                .trim_end_matches('/')
                .to_owned(),
            allowed_hackmd_users: optional("ALLOWED_HACKMD_USERS")
                .map(|v| {
                    v.split(',')
                        .map(|s| s.trim().to_lowercase())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            encryption_key,
            web_dist: optional("WEB_DIST")
                .unwrap_or_else(|| "web/dist".into())
                .into(),
        })
    }

    /// The canonical MCP resource identifier (RFC 8707 / RFC 9728).
    pub fn mcp_resource(&self) -> String {
        format!("{}/mcp", self.public_url)
    }

    /// The host name of `public_url`; empty only for a `Config` built by hand
    /// with an invalid URL.
    pub fn public_host(&self) -> String {
        Url::parse(&self.public_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_default()
    }

    /// Whether `value` names the MCP resource in an RFC 8707 `resource` parameter.
    /// Tolerates a trailing slash and the bare origin, which some clients send.
    pub fn is_mcp_resource(&self, value: &str) -> bool {
        let value = value.strip_suffix('/').unwrap_or(value);
        value == self.mcp_resource() || value == self.public_url
    }

    /// Cookies get the `Secure` attribute when the dashboard is served over https.
    pub fn secure_cookies(&self) -> bool {
        self.public_url.starts_with("https://")
    }

    /// Whether a browser `Origin` header names this server, compared as
    /// serialized origins so case and explicit default ports don't matter.
    pub fn is_own_origin(&self, origin: &str) -> bool {
        serialized_origin(origin).is_some_and(|o| Some(o) == serialized_origin(&self.public_url))
    }

    /// The dashboard allowlist rule: a HackMD user may sign in when the list
    /// is empty or names their user path or email. Queries that must re-check
    /// it for an existing session use [`hackmd_user_allowed_sql!`], its SQL twin.
    pub fn is_hackmd_user_allowed(&self, user_path: Option<&str>, email: Option<&str>) -> bool {
        self.allowed_hackmd_users.is_empty()
            || [user_path, email]
                .into_iter()
                .flatten()
                .any(|id| self.allowed_hackmd_users.contains(&id.to_lowercase()))
    }
}

/// SQL form of [`Config::is_hackmd_user_allowed`], for `concat!` into a query
/// that aliases `users` as `u` and binds `allowed_hackmd_users` as `$2`.
macro_rules! hackmd_user_allowed_sql {
    () => {
        "(cardinality($2::text[]) = 0 OR lower(u.user_path) = ANY($2) OR lower(u.email) = ANY($2))"
    };
}
pub(crate) use hackmd_user_allowed_sql;

/// `None` for unparsable URLs and opaque origins such as `null`.
fn serialized_origin(url: &str) -> Option<String> {
    let origin = Url::parse(url).ok()?.origin();
    origin.is_tuple().then(|| origin.ascii_serialization())
}

fn required(key: &str) -> Result<String> {
    match optional(key) {
        Some(v) => Ok(v),
        None => bail!("missing required environment variable {key}"),
    }
}

fn optional(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::serialized_origin;

    #[test]
    fn origins_compare_normalized() {
        let ours = serialized_origin("https://HackMD-MCP.nath.tw:443");
        assert_eq!(ours.as_deref(), Some("https://hackmd-mcp.nath.tw"));
        assert_eq!(serialized_origin("https://hackmd-mcp.nath.tw"), ours);
        assert_ne!(serialized_origin("http://hackmd-mcp.nath.tw"), ours);
        assert_ne!(serialized_origin("https://hackmd-mcp.nath.tw:8443"), ours);
        assert_eq!(serialized_origin("null"), None);
    }
}
