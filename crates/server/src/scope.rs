//! Which HackMD workspaces a session may touch.

use mcp_backbone::Scope;
use serde::{Deserialize, Serialize};

/// Stored per session. `teams` are snapshots taken when the user picked
/// them, so the dashboard can render them without asking HackMD. `{}` (a
/// new session) means every workspace.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceScope {
    #[serde(default)]
    pub mode: ScopeMode,
    /// With `allow`: whether the personal workspace is allowed.
    #[serde(default = "yes")]
    pub personal: bool,
    /// With `allow`: the allowed teams.
    #[serde(default)]
    pub teams: Vec<ScopedTeam>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScopeMode {
    /// Every workspace the HackMD account can reach.
    #[default]
    All,
    /// Only the listed ones.
    Allow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedTeam {
    /// The team path, as HackMD API routes use it.
    pub path: String,
    pub name: String,
}

impl WorkspaceScope {
    /// Whether the scope allows the team with `team_path`, or the personal
    /// workspace for `None`.
    pub fn allows(&self, team_path: Option<&str>) -> bool {
        match (self.mode, team_path) {
            (ScopeMode::All, _) => true,
            (ScopeMode::Allow, None) => self.personal,
            (ScopeMode::Allow, Some(path)) => self
                .teams
                .iter()
                .any(|team| team.path.eq_ignore_ascii_case(path)),
        }
    }

    /// What the agent is told when the scope refuses a workspace.
    pub fn denial(team_path: Option<&str>) -> String {
        match team_path {
            None => "This connection is not allowed to access the user's personal workspace. \
                     The user chooses which workspaces it may access in the dashboard."
                .to_owned(),
            Some(team) => format!(
                "This connection is not allowed to access the team workspace {team}. The user \
                 chooses which workspaces it may access in the dashboard."
            ),
        }
    }
}

impl Scope for WorkspaceScope {
    fn is_restricted(&self) -> bool {
        self.mode == ScopeMode::Allow
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn allow(personal: bool, teams: &[&str]) -> WorkspaceScope {
        WorkspaceScope {
            mode: ScopeMode::Allow,
            personal,
            teams: teams
                .iter()
                .map(|path| ScopedTeam {
                    path: (*path).into(),
                    name: path.to_uppercase(),
                })
                .collect(),
        }
    }

    #[test]
    fn workspace_scope_allows() {
        let all: WorkspaceScope =
            serde_json::from_value(json!({"mode": "all", "personal": true, "teams": []})).unwrap();
        assert!(all.allows(None) && all.allows(Some("any")));
        assert!(allow(true, &[]).allows(None));
        assert!(!allow(true, &[]).allows(Some("core")));
        assert!(!allow(false, &["core"]).allows(None));
        assert!(allow(false, &["core"]).allows(Some("Core")));
        assert!(!all.is_restricted() && allow(true, &[]).is_restricted());
    }

    #[test]
    fn new_and_legacy_sessions_parse() {
        // A session created by the framework stores `{}`.
        let new: WorkspaceScope = serde_json::from_value(json!({})).unwrap();
        assert_eq!(
            new,
            WorkspaceScope {
                personal: true,
                ..Default::default()
            }
        );
        assert!(new.allows(Some("x")));
        let value = json!({"mode": "allow", "personal": false, "teams": [{"path": "core", "name": "Core"}]});
        let parsed: WorkspaceScope = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap(), value);
        assert!(serde_json::from_value::<WorkspaceScope>(json!({"mode": "deny"})).is_err());
    }

    #[test]
    fn denials_name_the_workspace() {
        assert!(WorkspaceScope::denial(Some("other")).contains("team workspace other"));
        assert!(WorkspaceScope::denial(None).contains("personal workspace"));
    }
}
