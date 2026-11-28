import { ErrorAlert } from "@nat1an/mcp-backbone-dashboard"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@nat1an/mcp-backbone-dashboard/ui/card"
import { Checkbox } from "@nat1an/mcp-backbone-dashboard/ui/checkbox"
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@nat1an/mcp-backbone-dashboard/ui/field"
import { Skeleton } from "@nat1an/mcp-backbone-dashboard/ui/skeleton"
import { Switch } from "@nat1an/mcp-backbone-dashboard/ui/switch"
import { useTeams } from "./use-hackmd"
import { useUpdateSession } from "@nat1an/mcp-backbone-dashboard"
import type { AgentSession } from "@nat1an/mcp-backbone-dashboard"

import type { ScopedTeam, WorkspaceScope } from "./types"

export function SessionWorkspacesCard({
  session,
}: {
  session: AgentSession<Partial<WorkspaceScope>>
}) {
  const update = useUpdateSession<WorkspaceScope>(session.id)
  // A new session stores `{}`: every workspace.
  const scope = normalize(session.scope)
  const all = scope.mode === "all"

  function save(next: WorkspaceScope) {
    update.mutate({ scope: next })
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Workspaces</CardTitle>
        <CardDescription>
          Which of your HackMD workspaces this agent may read and change.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <FieldGroup>
          <Field orientation="horizontal">
            <FieldContent>
              <FieldLabel htmlFor="all-workspaces">All workspaces</FieldLabel>
              <FieldDescription>
                Your personal notes and every team you belong to, including
                teams you join later.
              </FieldDescription>
            </FieldContent>
            <Switch
              id="all-workspaces"
              checked={all}
              onCheckedChange={(checked) =>
                // Narrowing starts from everything currently reachable.
                save({ ...scope, mode: checked ? "all" : "allow" })
              }
            />
          </Field>
          {!all && <WorkspacePicker scope={scope} onChange={save} />}
        </FieldGroup>
      </CardContent>
    </Card>
  )
}

type WorkspacePickerProps = {
  scope: WorkspaceScope
  onChange: (scope: WorkspaceScope) => void
}

function WorkspacePicker({ scope, onChange }: WorkspacePickerProps) {
  const teams = useTeams()

  function setTeam(team: ScopedTeam, allowed: boolean) {
    const others = scope.teams.filter((t) => t.path !== team.path)
    onChange({ ...scope, teams: allowed ? [...others, team] : others })
  }

  // Teams picked earlier that the account has since left still show, so
  // they can be removed.
  const known = teams.data ?? []
  const stale = scope.teams.filter(
    (team) => !known.some((t) => t.path === team.path)
  )
  const choices: ScopedTeam[] = [
    ...known.map(({ path, name }) => ({ path, name })),
    ...stale,
  ]

  return (
    <FieldSet>
      <FieldLegend variant="label">Allowed workspaces</FieldLegend>
      <FieldGroup>
        <Field orientation="horizontal">
          <Checkbox
            id="workspace-personal"
            checked={scope.personal}
            onCheckedChange={(checked) =>
              onChange({ ...scope, personal: checked })
            }
          />
          <FieldLabel htmlFor="workspace-personal">
            My personal workspace
          </FieldLabel>
        </Field>
        {teams.isPending && <Skeleton className="h-6 w-48" />}
        {teams.isError && (
          <ErrorAlert title="Couldn't load your teams" error={teams.error} />
        )}
        {choices.map((team) => {
          const id = `workspace-team-${team.path}`
          return (
            <Field key={team.path} orientation="horizontal">
              <Checkbox
                id={id}
                checked={scope.teams.some((t) => t.path === team.path)}
                onCheckedChange={(checked) => setTeam(team, checked)}
              />
              <FieldLabel htmlFor={id}>
                {team.name}
                <span className="text-muted-foreground">/{team.path}</span>
              </FieldLabel>
            </Field>
          )
        })}
      </FieldGroup>
    </FieldSet>
  )
}

/** The scope with the server's defaults filled in. */
export function normalize(scope: Partial<WorkspaceScope>): WorkspaceScope {
  return {
    mode: scope.mode ?? "all",
    personal: scope.personal ?? true,
    teams: scope.teams ?? [],
  }
}
