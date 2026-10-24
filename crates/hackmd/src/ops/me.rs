//! The token owner.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Account, Result};

/// The parts of `GET /me` the server keeps.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub user_path: Option<String>,
    #[serde(default)]
    pub photo: Option<String>,
    #[serde(default)]
    pub teams: Vec<Team>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub logo: Option<String>,
}

impl Account {
    /// `GET /me`, as HackMD returns it.
    pub async fn get_me(&self) -> Result<Value> {
        self.get("me").await
    }

    /// `GET /me`, typed; also verifies the token.
    pub async fn me(&self) -> Result<Me> {
        self.get_as("me").await
    }
}
