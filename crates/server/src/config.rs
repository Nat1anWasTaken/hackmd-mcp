//! HackMD settings, read from the environment; the framework reads its own.

use mcp_backbone::config::optional;

/// The HackMD API root, without a trailing slash (`HACKMD_API_URL`).
pub fn api_url() -> String {
    optional("HACKMD_API_URL")
        .unwrap_or_else(|| "https://api.hackmd.io/v1".into())
        .trim_end_matches('/')
        .to_owned()
}
