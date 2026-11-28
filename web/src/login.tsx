import { useSearchParams } from "react-router"
import { nextPath } from "@nat1an/mcp-backbone-dashboard"

import { HackmdTokenForm } from "./hackmd-token-form"

/** The login page's content: paste a HackMD API token. */
export function Login() {
  const [searchParams] = useSearchParams()
  return (
    <HackmdTokenForm
      next={nextPath(searchParams)}
      submitLabel="Sign in"
      // A full navigation: `next` may be a server route such as
      // /oauth/authorize.
      onSuccess={(redirectTo) => window.location.assign(redirectTo)}
    />
  )
}
