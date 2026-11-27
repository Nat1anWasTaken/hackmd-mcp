//! The tool registry: one MCP tool per [`hackmd::Account`] operation. The ten
//! `hackmd_*` note and folder tools keep the names and parameters of the
//! original hackmd-mcp-proxy, so existing agents keep working.

use hackmd::Account;
use mcp_backbone::{Targets, Tool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::service::{HackMdService, Target};

/// Arguments of tools that take none.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoArgs {}

macro_rules! targets {
    ($target:ident => $($request:ty),* $(,)?) => {$(
        impl Targets<Target> for $request {
            fn targets(&self) -> Target {
                targets!(@ $target self)
            }
        }
    )*};
    (@ workspace $self:ident) => { Target::Workspace($self.workspace.team_path().map(str::to_owned)) };
    (@ note $self:ident) => { Target::Note($self.note_id.clone()) };
    (@ account $self:ident) => { Target::Account };
}

targets!(workspace =>
    hackmd::ListNotes, hackmd::NoteRef, hackmd::CreateNote, hackmd::EditNote, hackmd::UpdateNote,
    hackmd::WorkspaceOnly, hackmd::CreateFolder, hackmd::UpdateFolder, hackmd::FolderRef,
);
targets!(note =>
    hackmd::ListVersions, hackmd::GetVersion, hackmd::CompareVersions, hackmd::CreateVersion,
    hackmd::UpdateVersion,
);
targets!(account => NoArgs);

/// Every tool, in the order clients and the dashboard list them.
pub fn all() -> Vec<Tool<HackMdService>> {
    use mcp_backbone::Effect::*;
    vec![
        Tool::new(
            "hackmd_list_notes",
            Read,
            "List notes",
            "List HackMD notes from a personal or team workspace. Supports proxy-side metadata \
             search with query over title, description, tags, id, and shortId; this does not \
             search note body content.",
        )
        .run(|account: Account, request: hackmd::ListNotes| async move {
            account.list_notes(request).await
        }),
        Tool::new(
            "hackmd_get_note",
            Read,
            "Get a note",
            "Get one HackMD note with full content, metadata, and the exact patch_path to use \
             with hackmd_edit_note.",
        )
        .run(|account: Account, request: hackmd::NoteRef| async move {
            account.get_note(request).await
        }),
        Tool::new(
            "hackmd_create_note",
            Additive,
            "Create a note",
            "Create a HackMD note in a personal or team workspace.",
        )
        .run(|account: Account, request: hackmd::CreateNote| async move {
            account.create_note(request).await
        }),
        Tool::new(
            "hackmd_edit_note",
            Idempotent,
            "Edit a note",
            "Default tool for normal HackMD content edits. Prefer this over hackmd_update_note for \
             editing note bodies. Provide a Codex-style patch from hackmd_get_note.patch_path; the \
             server applies it only when context matches, and only if the note did not change \
             meanwhile.",
        )
        .run(|account: Account, request: hackmd::EditNote| async move {
            account.edit_note(request).await
        }),
        Tool::new(
            "hackmd_update_note",
            Idempotent,
            "Update a note",
            "Fallback/full HackMD note update tool. Do not use for normal content edits unless \
             hackmd_edit_note failed or is insufficient. Use this when changing metadata such as \
             title, tags, permissions, description, permalink, or parentFolderId, or when a full \
             content replacement is explicitly necessary.",
        )
        .run(|account: Account, request: hackmd::UpdateNote| async move {
            account.update_note(request).await
        }),
        Tool::new(
            "hackmd_delete_note",
            Destructive,
            "Delete a note",
            "Delete a HackMD note from a personal or team workspace.",
        )
        .run(|account: Account, request: hackmd::NoteRef| async move {
            account.delete_note(request).await
        }),
        Tool::new(
            "hackmd_list_folders",
            Read,
            "List folders",
            "List folders in a personal or team HackMD workspace.",
        )
        .run(
            |account: Account, request: hackmd::WorkspaceOnly| async move {
                account.list_folders(request).await
            },
        ),
        Tool::new(
            "hackmd_create_folder",
            Additive,
            "Create a folder",
            "Create a folder in a personal or team HackMD workspace.",
        )
        .run(
            |account: Account, request: hackmd::CreateFolder| async move {
                account.create_folder(request).await
            },
        ),
        Tool::new(
            "hackmd_update_folder",
            Idempotent,
            "Update a folder",
            "Update folder metadata in a personal or team HackMD workspace.",
        )
        .run(
            |account: Account, request: hackmd::UpdateFolder| async move {
                account.update_folder(request).await
            },
        ),
        Tool::new(
            "hackmd_delete_folder",
            Destructive,
            "Delete a folder",
            "Delete a folder from a personal or team HackMD workspace.",
        )
        .run(|account: Account, request: hackmd::FolderRef| async move {
            account.delete_folder(request).await
        }),
        Tool::new(
            "hackmd_get_me",
            Read,
            "Get my account",
            "Get the HackMD account these tools act as, including the teams it belongs to and \
             their team_path values for team workspaces.",
        )
        .run(|account: Account, _: NoArgs| async move { account.get_me().await }),
        Tool::new(
            "hackmd_list_versions",
            Read,
            "List note versions",
            "List saved versions of a note, newest first, with their names, authors and whether \
             their content is still available. Free workspaces keep content for the 10 most \
             recent versions only.",
        )
        .run(
            |account: Account, request: hackmd::ListVersions| async move {
                account.list_versions(request).await
            },
        ),
        Tool::new(
            "hackmd_get_version",
            Read,
            "Get a note version",
            "Get one saved version of a note, with its content when available.",
        )
        .run(|account: Account, request: hackmd::GetVersion| async move {
            account.get_version(request).await
        }),
        Tool::new(
            "hackmd_compare_versions",
            Read,
            "Compare note versions",
            "Get a unified diff between two saved versions of a note, or between a version and \
             the current note (`note_content`).",
        )
        .run(
            |account: Account, request: hackmd::CompareVersions| async move {
                account.compare_versions(request).await
            },
        ),
        Tool::new(
            "hackmd_create_version",
            Additive,
            "Name a note version",
            "Save the current note as a named version (a labelled snapshot, like a commit \
             message), or name an existing saved version with base `version:<id>`. Naming is not \
             an approval.",
        )
        .run(
            |account: Account, request: hackmd::CreateVersion| async move {
                account.create_version(request).await
            },
        ),
        Tool::new(
            "hackmd_update_version",
            Idempotent,
            "Rename a note version",
            "Change the name or description of a named version of a note.",
        )
        .run(
            |account: Account, request: hackmd::UpdateVersion| async move {
                account.update_version(request).await
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use mcp_backbone::mcp::{Call, Kind, Registry};
    use serde_json::{Value, json};

    use super::*;

    fn registry() -> Registry<HackMdService> {
        Registry::new(all(), false)
    }

    fn find(name: &str) -> Option<Tool<HackMdService>> {
        all().into_iter().find(|tool| tool.name == name)
    }

    fn parse(tool: &str, args: Value) -> serde_json::Result<Box<dyn Call<HackMdService>>> {
        let registry = registry();
        let tool = registry.find(tool).unwrap();
        registry.parse(tool, &args).unwrap()
    }

    fn properties(name: &str) -> BTreeSet<String> {
        let schema = find(name).unwrap().definition().input_schema;
        schema
            .get("properties")
            .and_then(Value::as_object)
            .map(|props| props.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn required(name: &str) -> BTreeSet<String> {
        let schema = find(name).unwrap().definition().input_schema;
        schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect()
    }

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn registry_is_consistent() {
        let names: HashSet<_> = all().iter().map(|tool| tool.name).collect();
        assert_eq!(names.len(), 16, "uniquely named");
        for definition in all().iter().map(Tool::definition) {
            assert_eq!(
                definition.input_schema["type"], "object",
                "{}",
                definition.name
            );
            assert!(definition.title.is_some() && definition.description.is_some());
        }
    }

    /// The contract of hackmd-mcp-proxy's tools: same names, same parameters.
    #[test]
    fn legacy_tools_keep_their_parameters() {
        let legacy: [(&str, &[&str], &[&str]); 10] = [
            (
                "hackmd_list_notes",
                &[
                    "workspace",
                    "query",
                    "tags",
                    "folder_id",
                    "limit",
                    "offset",
                    "sort",
                ],
                &[],
            ),
            ("hackmd_get_note", &["workspace", "note_id"], &["note_id"]),
            ("hackmd_create_note", &["workspace", "note"], &["note"]),
            (
                "hackmd_edit_note",
                &["workspace", "note_id", "patch"],
                &["note_id", "patch"],
            ),
            (
                "hackmd_update_note",
                &["workspace", "note_id", "fields"],
                &["note_id", "fields"],
            ),
            (
                "hackmd_delete_note",
                &["workspace", "note_id"],
                &["note_id"],
            ),
            ("hackmd_list_folders", &["workspace"], &[]),
            (
                "hackmd_create_folder",
                &["workspace", "folder"],
                &["folder"],
            ),
            (
                "hackmd_update_folder",
                &["workspace", "folder_id", "fields"],
                &["folder_id", "fields"],
            ),
            (
                "hackmd_delete_folder",
                &["workspace", "folder_id"],
                &["folder_id"],
            ),
        ];
        for (name, props, req) in legacy {
            assert_eq!(properties(name), set(props), "{name} properties");
            assert_eq!(required(name), set(req), "{name} required");
        }
    }

    #[test]
    fn kinds_and_annotations_follow_effects() {
        let annotations = |name: &str| find(name).unwrap().definition().annotations.unwrap();
        assert_eq!(find("hackmd_get_note").unwrap().kind, Kind::Read);
        assert_eq!(annotations("hackmd_list_notes").read_only_hint, Some(true));
        assert_eq!(find("hackmd_create_note").unwrap().kind, Kind::Write);
        assert_eq!(
            annotations("hackmd_create_note").idempotent_hint,
            Some(false)
        );
        assert_eq!(annotations("hackmd_edit_note").idempotent_hint, Some(true));
        for name in ["hackmd_delete_note", "hackmd_delete_folder"] {
            assert_eq!(annotations(name).destructive_hint, Some(true), "{name}");
        }
        assert_eq!(
            annotations("hackmd_edit_note").destructive_hint,
            Some(false)
        );
    }

    #[test]
    fn parses_arguments_into_requests() {
        let call = parse("hackmd_get_note", json!({ "note_id": "abc" })).unwrap();
        assert_eq!(call.target(), Target::Workspace(None));

        let args = json!({ "workspace": { "kind": "team", "team_path": "core" }, "note_id": "a" });
        let call = parse("hackmd_delete_note", args).unwrap();
        assert_eq!(call.target(), Target::Workspace(Some("core".into())));

        let call = parse("hackmd_list_versions", json!({ "note_id": "n1" })).unwrap();
        assert_eq!(call.target(), Target::Note("n1".into()));

        // Metadata fields are typed, and unknown ones pass through.
        let args =
            json!({ "note_id": "a", "fields": { "title": "T", "parentFolderId": null, "x": 1 } });
        let call = parse("hackmd_update_note", args).unwrap();
        assert_eq!(
            call.log_arguments()["fields"],
            json!({ "title": "T", "parentFolderId": null, "x": 1 })
        );

        let error = parse("hackmd_get_note", json!({})).err().unwrap();
        assert!(error.to_string().contains("missing field `note_id`"));
        let error = parse("hackmd_get_note", json!({ "note_id": "a", "bogus": 1 }))
            .err()
            .unwrap();
        assert!(error.to_string().contains("unknown field"));
        let error = parse(
            "hackmd_create_note",
            json!({ "note": { "readPermission": "everyone" } }),
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("unknown variant"));
    }
}
