//! Saved and named versions of a note.

use reqwest::Method;
use serde_json::{Value, json};

use crate::{
    Account, CompareVersions, CreateVersion, Error, GetVersion, ListVersions, Result,
    UpdateVersion, workspace::segment,
};

fn versions(note_id: &str) -> String {
    format!("notes/{}/versions", segment(note_id))
}

/// A version reference as the API takes it: `version:<id>`.
fn reference(version_id: &str) -> String {
    if version_id.starts_with("version:") {
        version_id.to_owned()
    } else {
        format!("version:{version_id}")
    }
}

/// The bare id of a `version:<id>` reference.
fn bare(version_id: &str) -> &str {
    version_id.strip_prefix("version:").unwrap_or(version_id)
}

impl Account {
    pub async fn list_versions(&self, request: ListVersions) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(named_only) = request.named_only {
            query.push(("named_only", named_only.to_string()));
        }
        for (key, value) in [
            ("q", &request.q),
            ("created_by", &request.created_by),
            ("created_after", &request.created_after),
            ("created_before", &request.created_before),
        ] {
            if let Some(value) = value {
                query.push((key, value.clone()));
            }
        }
        if let Some(page) = request.page {
            query.push(("page", page.to_string()));
        }
        if let Some(limit) = request.limit {
            query.push(("limit", limit.to_string()));
        }
        self.send(
            self.request(Method::GET, &versions(&request.note_id))
                .query(&query),
        )
        .await
    }

    pub async fn get_version(&self, request: GetVersion) -> Result<Value> {
        let path = format!(
            "{}/{}",
            versions(&request.note_id),
            segment(bare(&request.version_id))
        );
        self.get(&path).await
    }

    pub async fn compare_versions(&self, request: CompareVersions) -> Result<Value> {
        let path = format!("{}/compare", versions(&request.note_id));
        let query = [("base", request.base), ("target", request.target)];
        self.send(self.request(Method::GET, &path).query(&query))
            .await
    }

    pub async fn create_version(&self, request: CreateVersion) -> Result<Value> {
        let mut body = json!({ "name": request.name });
        if let Some(base) = &request.base {
            body["base"] = json!(base);
        }
        if let Some(description) = &request.description {
            body["description"] = json!(description);
        }
        self.post(&versions(&request.note_id), &body)
            .await
            .map_err(|err| match err {
                Error::Conflict(message) if message.contains("no_content_change") => {
                    Error::Conflict(
                        "the note has not changed since its latest named version".into(),
                    )
                }
                Error::Conflict(message) if message.contains("version_already_named") => {
                    Error::Conflict(
                        "that version already has a name; use hackmd_update_version to rename it"
                            .into(),
                    )
                }
                other => other,
            })
    }

    pub async fn update_version(&self, request: UpdateVersion) -> Result<Value> {
        if request.name.is_none() && request.description.is_none() {
            return Err(Error::InvalidRequest(
                "give a new name, a new description, or both".into(),
            ));
        }
        let mut body = json!({ "base": reference(&request.version_id) });
        if let Some(name) = &request.name {
            body["name"] = json!(name);
        }
        if let Some(description) = &request.description {
            body["description"] = json!(description);
        }
        self.patch(&versions(&request.note_id), &body)
            .await
            .map_err(|err| match err {
                Error::Rejected(message) if message.to_lowercase().contains("not named") => {
                    Error::Rejected(
                        "only named versions can be updated; name it with hackmd_create_version \
                         and base `version:<id>`"
                            .into(),
                    )
                }
                other => other,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_references() {
        assert_eq!(reference("abc"), "version:abc");
        assert_eq!(reference("version:abc"), "version:abc");
        assert_eq!(bare("version:abc"), "abc");
    }
}
