//! Notes in a personal or team workspace.

use std::cmp::Reverse;

use serde::Serialize;
use serde_json::{Value, json};

use super::folders::is_empty;
use crate::{
    Account, CreateNote, DEFAULT_LIST_LIMIT, EditNote, Error, ListNotes, MAX_LIST_LIMIT, NoteRef,
    NoteSort, Result, UpdateNote, Workspace, patch,
};

#[derive(Debug, Serialize)]
pub struct NoteList {
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub notes: Vec<NoteSummary>,
}

#[derive(Debug, Serialize)]
pub struct NoteSummary {
    pub id: String,
    pub short_id: Option<String>,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub workspace: Workspace,
    pub patch_path: String,
    pub folder_ids: Vec<String>,
    pub created_at: i64,
    pub last_changed_at: i64,
    pub publish_link: Option<String>,
    pub permalink: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NoteWithContent {
    pub note_id: String,
    pub patch_path: String,
    pub content: Option<String>,
    pub note: Value,
}

#[derive(Debug, Serialize)]
pub struct EditedNote {
    pub note_id: String,
    pub patch_path: String,
    pub changed: bool,
    pub content: String,
}

impl Account {
    /// Every note in the workspace, filtered, sorted and paged here: HackMD
    /// has no server-side search.
    pub async fn list_notes(&self, request: ListNotes) -> Result<NoteList> {
        let workspace = request.workspace;
        let Value::Array(notes) = self.get(&workspace.notes()).await? else {
            return Err(Error::BadResponse("expected a list of notes".into()));
        };
        let mut notes: Vec<NoteSummary> = notes
            .into_iter()
            .map(|note| NoteSummary::new(&workspace, &note))
            .collect();

        if let Some(query) = request
            .query
            .as_deref()
            .map(normalize)
            .filter(|query| !query.is_empty())
        {
            notes.retain(|note| note.matches(&query));
        }
        if !request.tags.is_empty() {
            let wanted: Vec<String> = request.tags.iter().map(|tag| tag.to_lowercase()).collect();
            notes.retain(|note| {
                wanted
                    .iter()
                    .all(|tag| note.tags.iter().any(|have| have.to_lowercase() == *tag))
            });
        }
        if let Some(folder_id) = request.folder_id.as_deref() {
            notes.retain(|note| note.folder_ids.iter().any(|id| id == folder_id));
        }

        match request.sort.unwrap_or_default() {
            NoteSort::LastChangedDesc => notes.sort_by_key(|note| Reverse(note.last_changed_at)),
            NoteSort::LastChangedAsc => notes.sort_by_key(|note| note.last_changed_at),
            NoteSort::CreatedDesc => notes.sort_by_key(|note| Reverse(note.created_at)),
            NoteSort::CreatedAsc => notes.sort_by_key(|note| note.created_at),
            NoteSort::TitleAsc => notes.sort_by_key(|note| note.title.to_lowercase()),
        }

        let total = notes.len();
        let offset = request.offset.unwrap_or(0).min(total);
        let limit = request
            .limit
            .unwrap_or(DEFAULT_LIST_LIMIT)
            .clamp(1, MAX_LIST_LIMIT);
        let notes = notes.into_iter().skip(offset).take(limit).collect();
        Ok(NoteList {
            total,
            offset,
            limit,
            notes,
        })
    }

    pub async fn get_note(&self, request: NoteRef) -> Result<NoteWithContent> {
        let note = self.get(&request.workspace.note(&request.note_id)).await?;
        Ok(NoteWithContent {
            patch_path: request.workspace.patch_path(&request.note_id),
            content: content(&note).ok(),
            note_id: request.note_id,
            note,
        })
    }

    pub async fn create_note(&self, request: CreateNote) -> Result<Value> {
        self.post(&request.workspace.notes(), &request.note).await
    }

    /// Applies a patch to the note's current content. The note is written
    /// only if the patch changes it, and only if nobody changed it meanwhile.
    pub async fn edit_note(&self, request: EditNote) -> Result<EditedNote> {
        let path = request.workspace.note(&request.note_id);
        let response = self
            .request(reqwest::Method::GET, &path)
            .send()
            .await
            .map_err(Error::network)?;
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let note = self.send_response(response).await?;
        let current = content(&note)?;
        let patch_path = request.workspace.patch_path(&request.note_id);
        let updated = patch::apply_note_patch(&current, &request.patch, &patch_path)?;
        let changed = updated != current;
        if changed {
            let unchanged = match self.get_if_changed(&path, etag.as_deref()).await? {
                None => true,
                Some(latest) => content(&latest)? == current,
            };
            if !unchanged {
                return Err(Error::Conflict(
                    "the note changed while the patch was being applied; read it again with \
                     hackmd_get_note and rebuild the patch"
                        .into(),
                ));
            }
            self.patch(&path, &json!({ "content": updated })).await?;
        }
        Ok(EditedNote {
            note_id: request.note_id,
            patch_path,
            changed,
            content: updated,
        })
    }

    pub async fn update_note(&self, request: UpdateNote) -> Result<Value> {
        if is_empty(&request.fields) {
            return Err(Error::InvalidRequest(
                "fields must contain at least one note property".into(),
            ));
        }
        self.patch(&request.workspace.note(&request.note_id), &request.fields)
            .await
    }

    pub async fn delete_note(&self, request: NoteRef) -> Result<Value> {
        self.delete(&request.workspace.note(&request.note_id)).await
    }

    /// The path of the team that owns the note, or `None` for a personal note.
    pub async fn note_team(&self, note_id: &str) -> Result<Option<String>> {
        let note = self.get(&Workspace::Personal.note(note_id)).await?;
        Ok(note
            .get("teamPath")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
            .map(str::to_owned))
    }
}

impl NoteSummary {
    fn new(workspace: &Workspace, note: &Value) -> Self {
        let id = string(note, "id").unwrap_or_default();
        Self {
            short_id: string(note, "shortId"),
            title: string(note, "title").unwrap_or_default(),
            description: string(note, "description").unwrap_or_default(),
            tags: note
                .get("tags")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|tag| tag.as_str().map(str::to_owned))
                .collect(),
            patch_path: workspace.patch_path(&id),
            folder_ids: note
                .get("folderPaths")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|folder| folder.get("id")?.as_str().map(str::to_owned))
                .collect(),
            created_at: number(note, "createdAt"),
            last_changed_at: number(note, "lastChangedAt"),
            publish_link: string(note, "publishLink"),
            permalink: string(note, "permalink"),
            workspace: workspace.clone(),
            id,
        }
    }

    fn matches(&self, query: &str) -> bool {
        [
            Some(&self.id),
            self.short_id.as_ref(),
            Some(&self.title),
            Some(&self.description),
        ]
        .into_iter()
        .flatten()
        .chain(&self.tags)
        .any(|field| normalize(field).contains(query))
    }
}

/// The note's Markdown body.
fn content(note: &Value) -> Result<String> {
    ["content", "text", "markdown"]
        .iter()
        .find_map(|field| note.get(*field)?.as_str().map(str::to_owned))
        .ok_or_else(|| Error::BadResponse("the note has no editable content".into()))
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

fn string(value: &Value, field: &str) -> Option<String> {
    value.get(field)?.as_str().map(str::to_owned)
}

fn number(value: &Value, field: &str) -> i64 {
    value
        .get(field)
        .and_then(|value| value.as_i64().or_else(|| value.as_f64().map(|n| n as i64)))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_includes_patch_path_and_folder_ids() {
        let summary = NoteSummary::new(
            &Workspace::Personal,
            &json!({
                "id": "abc",
                "shortId": "short",
                "title": "Roadmap",
                "description": "Q3",
                "tags": ["planning"],
                "createdAt": 1,
                "lastChangedAt": 2,
                "folderPaths": [{ "id": "folder-1" }]
            }),
        );
        assert_eq!(summary.patch_path, "notes/abc.md");
        assert_eq!(summary.folder_ids, vec!["folder-1"]);
        assert!(summary.matches("road"));
        assert!(summary.matches("planning"));
        assert!(!summary.matches("nothing"));
    }
}
