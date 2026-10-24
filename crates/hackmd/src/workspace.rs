//! Personal and team workspaces, and the API paths inside them.

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where notes and folders live. Defaults to `{"kind": "personal"}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Workspace {
    /// The token owner's own notes.
    #[default]
    Personal,
    /// A team's notes; `team_path` is the team's path, as `hackmd_get_me` lists it.
    Team { team_path: String },
}

impl Workspace {
    /// The team's path, or `None` for the personal workspace.
    pub fn team_path(&self) -> Option<&str> {
        match self {
            Self::Personal => None,
            Self::Team { team_path } => Some(team_path),
        }
    }

    fn prefix(&self) -> String {
        match self {
            Self::Personal => String::new(),
            Self::Team { team_path } => format!("teams/{}/", segment(team_path)),
        }
    }

    pub(crate) fn notes(&self) -> String {
        format!("{}notes", self.prefix())
    }

    pub(crate) fn note(&self, note_id: &str) -> String {
        format!("{}notes/{}", self.prefix(), segment(note_id))
    }

    pub(crate) fn folders(&self) -> String {
        format!("{}folders", self.prefix())
    }

    pub(crate) fn folder(&self, folder_id: &str) -> String {
        format!("{}folders/{}", self.prefix(), segment(folder_id))
    }

    /// The file path `hackmd_edit_note` patches must target.
    pub fn patch_path(&self, note_id: &str) -> String {
        format!("{}notes/{}.md", self.prefix(), segment(note_id))
    }
}

/// Characters that would end or change the meaning of a path segment.
const SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'/')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'\\')
    .add(b'`')
    .add(b'{')
    .add(b'}');

/// Percent-encodes one path segment.
pub(crate) fn segment(value: &str) -> String {
    utf8_percent_encode(value, SEGMENT).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        let team = Workspace::Team {
            team_path: "core-team".into(),
        };
        assert_eq!(Workspace::Personal.notes(), "notes");
        assert_eq!(team.notes(), "teams/core-team/notes");
        assert_eq!(Workspace::Personal.note("a/b"), "notes/a%2Fb");
        assert_eq!(Workspace::Personal.patch_path("abc"), "notes/abc.md");
        assert_eq!(team.folder("f1"), "teams/core-team/folders/f1");
    }
}
