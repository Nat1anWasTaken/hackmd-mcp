//! Permission decisions over a session's settings. Free of I/O so every rule
//! is unit tested.

use std::fmt;

use super::tools::{Kind, Tool};
use crate::sessions::{Permissions, WorkspaceScope};

/// Why a call may not run. Displays as the message the agent receives.
#[derive(Debug, PartialEq, Eq)]
pub enum Denial {
    Disabled,
    NotAllowed(&'static str),
    ReadOnly,
    /// The team path, or `None` for the personal workspace.
    Workspace(Option<String>),
}

impl fmt::Display for Denial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => f.write_str("This connection is disabled in the dashboard."),
            Self::NotAllowed(tool) => write!(
                f,
                "The user has not allowed this connection to use {tool}. They can change that \
                 in the dashboard."
            ),
            Self::ReadOnly => f.write_str(
                "This connection is read-only, so it cannot change anything in HackMD. The user \
                 can allow write tools in the dashboard.",
            ),
            Self::Workspace(None) => f.write_str(
                "This connection is not allowed to access the user's personal workspace. The \
                 user chooses which workspaces it may access in the dashboard.",
            ),
            Self::Workspace(Some(team)) => write!(
                f,
                "This connection is not allowed to access the team workspace {team}. The user \
                 chooses which workspaces it may access in the dashboard."
            ),
        }
    }
}

/// Whether the session may call the tool at all, judging by the session
/// alone. `tools/list` shows exactly the tools that pass.
pub fn check_tool(session: &Permissions, tool: &Tool) -> Result<(), Denial> {
    if !session.enabled {
        return Err(Denial::Disabled);
    }
    let allowed = session
        .allowed_tools
        .as_ref()
        .is_none_or(|allowed| allowed.iter().any(|name| name == tool.name));
    if !allowed {
        Err(Denial::NotAllowed(tool.name))
    } else if tool.kind == Kind::Write && session.read_only {
        Err(Denial::ReadOnly)
    } else {
        Ok(())
    }
}

/// Whether the scope allows the team with `team_path`, or the personal
/// workspace for `None`.
pub fn check_workspace(scope: &WorkspaceScope, team_path: Option<&str>) -> Result<(), Denial> {
    if scope.allows(team_path) {
        Ok(())
    } else {
        Err(Denial::Workspace(team_path.map(str::to_owned)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mcp::tools::find,
        sessions::{ScopeMode, ScopedTeam},
    };

    fn session() -> Permissions {
        Permissions {
            enabled: true,
            read_only: false,
            allowed_tools: None,
            workspace_scope: WorkspaceScope {
                mode: ScopeMode::All,
                personal: true,
                teams: Vec::new(),
            },
        }
    }

    #[test]
    fn tool_checks() {
        let read = find("hackmd_get_note").unwrap();
        let write = find("hackmd_edit_note").unwrap();
        assert_eq!(check_tool(&session(), read), Ok(()));
        assert_eq!(check_tool(&session(), write), Ok(()));

        let disabled = Permissions {
            enabled: false,
            ..session()
        };
        for tool in [read, write] {
            assert_eq!(check_tool(&disabled, tool), Err(Denial::Disabled));
        }

        let read_only = Permissions {
            read_only: true,
            ..session()
        };
        assert_eq!(check_tool(&read_only, read), Ok(()));
        assert_eq!(check_tool(&read_only, write), Err(Denial::ReadOnly));

        let restricted = Permissions {
            allowed_tools: Some(vec!["hackmd_edit_note".into()]),
            ..session()
        };
        assert_eq!(check_tool(&restricted, write), Ok(()));
        assert_eq!(
            check_tool(&restricted, read),
            Err(Denial::NotAllowed("hackmd_get_note"))
        );
    }

    #[test]
    fn workspace_checks_name_the_workspace() {
        let scope = WorkspaceScope {
            mode: ScopeMode::Allow,
            personal: false,
            teams: vec![ScopedTeam {
                path: "core".into(),
                name: "Core".into(),
            }],
        };
        assert_eq!(check_workspace(&scope, Some("core")), Ok(()));
        let denied = check_workspace(&scope, Some("other"));
        assert_eq!(denied, Err(Denial::Workspace(Some("other".into()))));
        assert!(
            denied
                .unwrap_err()
                .to_string()
                .contains("team workspace other")
        );
        let denied = check_workspace(&scope, None).unwrap_err();
        assert!(denied.to_string().contains("personal workspace"));
        assert_eq!(
            check_workspace(&session().workspace_scope, Some("x")),
            Ok(())
        );
    }
}
