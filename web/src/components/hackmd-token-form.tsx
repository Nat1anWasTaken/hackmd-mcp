import { useState, type FormEvent } from "react"

import { ErrorAlert } from "@/components/error-alert"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { useHackmdSignIn } from "@/hooks/use-hackmd"

export const TOKEN_SETTINGS_URL = "https://hackmd.io/settings#api"

type HackmdTokenFormProps = {
  /** Where to continue after signing in; a local path. */
  next?: string
  submitLabel: string
  onSuccess: (redirectTo: string) => void
}

export function HackmdTokenForm({
  next,
  submitLabel,
  onSuccess,
}: HackmdTokenFormProps) {
  const [token, setToken] = useState("")
  const signIn = useHackmdSignIn()

  function submit(event: FormEvent) {
    event.preventDefault()
    signIn.mutate(
      { token: token.trim(), next },
      {
        onSuccess: ({ redirect_to }) => {
          setToken("")
          onSuccess(redirect_to)
        },
      }
    )
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <Field>
        <FieldLabel htmlFor="hackmd-token">HackMD API token</FieldLabel>
        <Input
          id="hackmd-token"
          type="password"
          autoComplete="off"
          spellCheck={false}
          value={token}
          onChange={(event) => setToken(event.target.value)}
          placeholder="Paste your token"
          required
        />
        <FieldDescription>
          Create one under{" "}
          <a
            href={TOKEN_SETTINGS_URL}
            target="_blank"
            rel="noreferrer"
            className="underline underline-offset-4"
          >
            HackMD settings → API
          </a>
          . It is stored encrypted and used only for your agents' requests.
        </FieldDescription>
      </Field>
      {signIn.error && <ErrorAlert error={signIn.error} />}
      <Button type="submit" disabled={signIn.isPending || !token.trim()}>
        {submitLabel}
      </Button>
    </form>
  )
}
