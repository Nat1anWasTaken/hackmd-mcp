// Shapes of the server's HackMD-specific API.

export type LinkStatus = {
  /** `needs_relink` once HackMD rejected the token. */
  status: "ok" | "needs_relink"
  last_ok_at: string | null
  last_error: string | null
  updated_at: string
}
export type Team = { path: string; name: string; logo: string | null }

export type ScopedTeam = { path: string; name: string }

/** Which workspaces a session may touch. With `all`, the rest is ignored. */
export type WorkspaceScope = {
  mode: "all" | "allow"
  personal: boolean
  teams: ScopedTeam[]
}
