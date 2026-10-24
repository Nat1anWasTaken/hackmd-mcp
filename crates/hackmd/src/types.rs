//! Tool requests. Doc comments become the MCP input schema descriptions;
//! field names are the tool contract and must not change.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::Workspace;

pub const DEFAULT_LIST_LIMIT: usize = 50;
pub const MAX_LIST_LIMIT: usize = 200;

/// Deserializes a field that may be absent (`None`), `null` (`Some(None)`) or
/// set, so updates can clear it.
fn nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer).map(Some)
}

// ----- notes ---------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NoteSort {
    #[default]
    LastChangedDesc,
    LastChangedAsc,
    CreatedDesc,
    CreatedAsc,
    TitleAsc,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListNotes {
    #[serde(default)]
    pub workspace: Workspace,
    /// Case-insensitive metadata search over title, description, tags, id, and
    /// shortId. Does not search note bodies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// Require every provided tag.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Only notes in this folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_id: Option<String>,
    /// 1-200, default 50.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 200))]
    pub limit: Option<usize>,
    /// Default 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
    /// Default last_changed_desc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<NoteSort>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoteRef {
    #[serde(default)]
    pub workspace: Workspace,
    pub note_id: String,
}

/// Who may read or write a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotePermission {
    Owner,
    SignedIn,
    Guest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CommentPermission {
    Disabled,
    Forbidden,
    Owners,
    SignedInUsers,
    Everyone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SuggestEditPermission {
    Disabled,
    Forbidden,
    Owners,
    SignedInUsers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FeatureAccess {
    Disabled,
    SignedInUsers,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct NoteFeatures {
    #[serde(
        rename = "emoji-reply",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub emoji_reply: Option<FeatureAccess>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citation: Option<FeatureAccess>,
}

/// HackMD note fields for a new note. Other HackMD fields are passed through.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewNote {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Markdown body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Folder to create the note in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_folder_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permalink: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_permission: Option<NotePermission>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_permission: Option<NotePermission>,
    /// Can only be set when creating the note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment_permission: Option<CommentPermission>,
    /// Can only be set when creating the note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggest_edit_permission: Option<SuggestEditPermission>,
    /// Can only be set when creating the note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_features: Option<NoteFeatures>,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateNote {
    #[serde(default)]
    pub workspace: Workspace,
    /// HackMD note fields such as title, content, tags, description,
    /// readPermission, writePermission, permalink, and parentFolderId.
    pub note: NewNote,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditNote {
    #[serde(default)]
    pub workspace: Workspace,
    pub note_id: String,
    /// Codex-style patch block with *** Begin Patch, *** Update File:
    /// <patch_path>, hunks, and *** End Patch. Each hunk starts with `@@`
    /// (optionally `@@ <unique line above the change>`), then lines prefixed
    /// with a space (context), `-` (remove) or `+` (add). A hunk with only `+`
    /// lines appends to the note, or inserts after its `@@` line. End a hunk
    /// with `*** End of File` to match at the end of the note.
    pub patch: String,
}

/// HackMD note PATCH fields. Other HackMD fields are passed through.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NoteChanges {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Replaces the whole body. Fallback only: prefer hackmd_edit_note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// `null` removes the description.
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub description: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// `null` moves the note out of its folder.
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub parent_folder_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permalink: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_permission: Option<NotePermission>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_permission: Option<NotePermission>,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateNote {
    #[serde(default)]
    pub workspace: Workspace,
    pub note_id: String,
    /// HackMD PATCH fields. May include metadata fields and, as fallback only,
    /// content.
    pub fields: NoteChanges,
}

// ----- folders -------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceOnly {
    #[serde(default)]
    pub workspace: Workspace,
}

/// HackMD folder fields. Other HackMD fields are passed through.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FolderFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `null` clears it on update.
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub description: Option<Option<String>>,
    /// Parent folder; `null` moves the folder to the top level on update.
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub parent_folder_id: Option<Option<String>>,
    /// Hex color: #RGB, #RGBA, #RRGGBB or #RRGGBBAA. `null` clears it on update.
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub color: Option<Option<String>>,
    /// Emoji code point, such as `1F525`. `null` clears it on update.
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub icon: Option<Option<String>>,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateFolder {
    #[serde(default)]
    pub workspace: Workspace,
    /// HackMD folder fields such as name, description, icon, color, and
    /// parentFolderId.
    pub folder: FolderFields,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateFolder {
    #[serde(default)]
    pub workspace: Workspace,
    pub folder_id: String,
    /// HackMD folder PATCH fields.
    pub fields: FolderFields,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FolderRef {
    #[serde(default)]
    pub workspace: Workspace,
    pub folder_id: String,
}

// ----- versions ------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListVersions {
    pub note_id: String,
    /// Only versions someone named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub named_only: Option<bool>,
    /// Free-text search over version names and descriptions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    /// Only versions created by this user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// ISO-8601 datetime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_after: Option<String>,
    /// ISO-8601 datetime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_before: Option<String>,
    /// Default 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub page: Option<u32>,
    /// Default 50.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetVersion {
    pub note_id: String,
    /// A version `id` from hackmd_list_versions.
    pub version_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompareVersions {
    pub note_id: String,
    /// `note_content` for the current note, or `version:<id>`.
    pub base: String,
    /// `note_content` for the current note, or `version:<id>`.
    pub target: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateVersion {
    pub note_id: String,
    /// 1-80 characters.
    #[schemars(length(min = 1, max = 80))]
    pub name: String,
    /// What to name: `note_content` (default) snapshots the current note;
    /// `version:<id>` names an existing saved version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Up to 500 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 500))]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateVersion {
    pub note_id: String,
    /// The named version to change, as an `id` from hackmd_list_versions.
    pub version_id: String,
    /// 1-80 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 80))]
    pub name: Option<String>,
    /// Up to 500 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 500))]
    pub description: Option<String>,
}
