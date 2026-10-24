//! Folders in a personal or team workspace.

use serde_json::Value;

use crate::{
    Account, CreateFolder, Error, FolderRef, Result, UpdateFolder, Workspace, WorkspaceOnly,
};

impl Account {
    pub async fn list_folders(&self, request: WorkspaceOnly) -> Result<Value> {
        self.get(&request.workspace.folders()).await
    }

    pub async fn create_folder(&self, request: CreateFolder) -> Result<Value> {
        self.post(&request.workspace.folders(), &request.folder)
            .await
    }

    pub async fn update_folder(&self, request: UpdateFolder) -> Result<Value> {
        if is_empty(&request.fields) {
            return Err(Error::InvalidRequest(
                "fields must contain at least one folder property".into(),
            ));
        }
        self.patch(
            &folder(&request.workspace, &request.folder_id),
            &request.fields,
        )
        .await
    }

    pub async fn delete_folder(&self, request: FolderRef) -> Result<Value> {
        self.delete(&folder(&request.workspace, &request.folder_id))
            .await
    }
}

fn folder(workspace: &Workspace, folder_id: &str) -> String {
    workspace.folder(folder_id)
}

/// Whether `fields` serializes to an empty object.
pub(crate) fn is_empty(fields: &impl serde::Serialize) -> bool {
    serde_json::to_value(fields)
        .ok()
        .and_then(|value| value.as_object().map(serde_json::Map::is_empty))
        .unwrap_or(true)
}
