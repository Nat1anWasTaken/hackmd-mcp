import { CircleAlertIcon } from "lucide-react"
import { useState } from "react"
import { toast } from "sonner"

import { HackmdTokenForm } from "@/components/hackmd-token-form"
import { RelativeTime } from "@/components/relative-time"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { useDisconnect } from "@/hooks/use-hackmd"
import { useSignedInMe } from "@/hooks/use-me"

export function HackmdCard() {
  const { user, hackmd } = useSignedInMe()
  const needsRelink = hackmd?.status !== "ok"
  const [replacing, setReplacing] = useState(false)
  const showForm = needsRelink || replacing

  return (
    <Card>
      <CardHeader>
        <CardTitle>HackMD account</CardTitle>
        <CardDescription>
          Your agents act as this account, with the token you signed in with.
        </CardDescription>
        <CardAction>
          {needsRelink ? (
            <Badge variant="destructive">Token rejected</Badge>
          ) : (
            <Badge variant="secondary">Connected</Badge>
          )}
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="flex items-center gap-3">
          <Avatar className="size-10">
            {user.avatar_url && <AvatarImage src={user.avatar_url} alt="" />}
            <AvatarFallback>
              {user.name.slice(0, 2).toUpperCase()}
            </AvatarFallback>
          </Avatar>
          <div className="flex min-w-0 flex-col">
            <span className="truncate font-medium">{user.name}</span>
            <span className="truncate text-sm text-muted-foreground">
              {[user.user_path && `@${user.user_path}`, user.email]
                .filter(Boolean)
                .join(" · ")}
            </span>
          </div>
        </div>
        {hackmd?.last_ok_at && (
          <p className="text-sm text-muted-foreground">
            Last used <RelativeTime value={hackmd.last_ok_at} />
          </p>
        )}
        {needsRelink && (
          <Alert variant="destructive">
            <CircleAlertIcon />
            <AlertTitle>HackMD rejected your token</AlertTitle>
            <AlertDescription>
              It was probably deleted or expired. Paste a new token for this
              account so your agents can work again.
            </AlertDescription>
          </Alert>
        )}
        {showForm ? (
          <HackmdTokenForm
            submitLabel="Save token"
            onSuccess={() => {
              setReplacing(false)
              toast.success("Token saved")
            }}
          />
        ) : (
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" onClick={() => setReplacing(true)}>
              Replace token
            </Button>
            <DisconnectButton />
          </div>
        )}
      </CardContent>
    </Card>
  )
}

function DisconnectButton() {
  const disconnect = useDisconnect()
  return (
    <AlertDialog>
      <AlertDialogTrigger render={<Button variant="destructive" />}>
        Delete account
      </AlertDialogTrigger>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete your account here?</AlertDialogTitle>
          <AlertDialogDescription>
            This removes your stored HackMD token, disconnects every agent and
            deletes your activity log. Your HackMD notes are not affected. You
            can sign in again with a token at any time.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            disabled={disconnect.isPending}
            onClick={() =>
              disconnect.mutate(undefined, {
                onError: (error) => toast.error(error.message),
              })
            }
          >
            Delete
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
