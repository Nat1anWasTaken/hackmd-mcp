import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useNavigate } from "react-router"

import { meQueryKey } from "@nat1an/mcp-backbone-dashboard"
import { api } from "@nat1an/mcp-backbone-dashboard"

import type { Team } from "./types"

/**
 * Signs in with a HackMD API token, or replaces the stored token when already
 * signed in. Resolves to the local path to continue to.
 */
export function useHackmdSignIn() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (body: { token: string; next?: string }) =>
      api.post<{ redirect_to: string }>("/auth/hackmd", body),
    onSuccess: () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: meQueryKey }),
        queryClient.invalidateQueries({ queryKey: teamsQueryKey }),
      ]),
  })
}

const teamsQueryKey = ["hackmd-teams"] as const

/** The user's teams, live from HackMD. */
export function useTeams(enabled = true) {
  return useQuery({
    queryKey: teamsQueryKey,
    queryFn: () => api.get<Team[]>("/api/hackmd/teams"),
    enabled,
    staleTime: 60_000,
  })
}

/** Deletes the user's account here: token, sessions and activity. */
export function useDisconnect() {
  const queryClient = useQueryClient()
  const navigate = useNavigate()
  return useMutation({
    mutationFn: () => api.delete("/api/hackmd"),
    onSuccess: () => {
      queryClient.clear()
      void navigate("/login")
    },
  })
}
