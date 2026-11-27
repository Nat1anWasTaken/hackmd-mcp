import {
  mountDashboard,
  type AgentSession,
  type Me,
} from "@nat1an/mcp-backbone-dashboard"
import { Badge } from "@nat1an/mcp-backbone-dashboard/ui/badge"

import "./index.css"
import { HackmdCard } from "./hackmd-card"
import { Login } from "./login"
import { normalize, SessionWorkspacesCard } from "./session-workspaces-card"
import type { LinkStatus, WorkspaceScope } from "./types"

mountDashboard({
  name: "HackMD",
  logo: "/hackmd.png",
  login: Login,
  loginDescription:
    "Sign in with a HackMD API token to choose which AI agents can use your notes.",
  linkHealth: (me: Me<unknown>) => {
    const link = me.link as LinkStatus | null
    if (link?.status === "ok") {
      return { ok: true }
    }
    return {
      ok: false,
      title: link ? "HackMD rejected your token" : "No HackMD token is saved",
      description:
        "Agents can't reach your notes until you save a new HackMD API token on the overview.",
    }
  },
  overviewCards: [HackmdCard],
  sessionCards: [SessionWorkspacesCard],
  sessionBadges: (session: AgentSession<Partial<WorkspaceScope>>) =>
    normalize(session.scope).mode === "allow" && (
      <Badge variant="outline">Limited workspaces</Badge>
    ),
  copy: {
    readOnlyDescription:
      "Blocks every tool that creates, edits or deletes notes, folders or versions.",
  },
})
