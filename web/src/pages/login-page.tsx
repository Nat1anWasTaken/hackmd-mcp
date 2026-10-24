import { useSearchParams } from "react-router"

import { HackmdTokenForm } from "@/components/hackmd-token-form"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"

export function LoginPage() {
  const [searchParams] = useSearchParams()
  const next = searchParams.get("next") ?? "/"

  return (
    <div className="flex min-h-svh items-center justify-center p-6">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <img src="/hackmd.png" alt="" className="size-8 rounded-md" />
            HackMD MCP
          </CardTitle>
          <CardDescription>
            Sign in with a HackMD API token to choose which AI agents can use
            your notes.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <HackmdTokenForm
            next={next}
            submitLabel="Sign in"
            // A full navigation: `next` may be a server route such as
            // /oauth/authorize.
            onSuccess={(redirectTo) => window.location.assign(redirectTo)}
          />
        </CardContent>
      </Card>
    </div>
  )
}
