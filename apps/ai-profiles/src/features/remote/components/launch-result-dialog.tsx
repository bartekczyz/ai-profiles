import type { RemoteHost, RemoteLaunch, WindowScreen } from '@/lib/types'

import { useState } from 'react'

import { useQueryClient } from '@tanstack/react-query'
import { Copy, SquareTerminal } from 'lucide-react'

import { Button, Dialog, Kbd, StatusDot, useToast } from '@/design'
import { sessionErrorMessage } from '@/features/profiles/components/session-error-message'
import { copyToClipboard, remoteOpenInTerminal } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

import { RemoteWindow } from './remote-window'

type Props = {
  host: RemoteHost
  account: string
  launch: RemoteLaunch
  onClose: () => void
}

/** A host name ssh takes as one plain word. */
const PLAIN_HOSTNAME = /^(?![-.])[A-Za-z0-9.-]{1,253}$/
/** Exactly what the server sends, from plain names and a window id only. */
const PLAIN_TMUX_ATTACH = /^tmux (-L [A-Za-z0-9_-]{1,64} )?attach -t [A-Za-z0-9_-]{1,64} \\; select-window -t @\d{1,9}$/

/**
 * `ssh -t <host> '<attach command>'`, to paste into a terminal on the Mac, or
 * null when either part is more than a plain name: both come from the host,
 * and whatever is copied here may be pasted into a shell. The same check as
 * Terminal's, in Rust.
 */
export function sshAttach(host: RemoteHost, attachCommand: string): string | null {
  if (!PLAIN_HOSTNAME.test(host.hostname) || !PLAIN_TMUX_ATTACH.test(attachCommand)) {
    return null
  }
  return `ssh -t ${host.hostname} '${attachCommand}'`
}

/**
 * Where a session started, and anything its window is waiting on. Shown when
 * there's something to say beyond "it started": a prompt to answer (with the
 * window live, to answer it here), or it was running already.
 */
export function LaunchResultDialog({ host, account, launch, onClose }: Props) {
  const queryClient = useQueryClient()
  const toast = useToast()
  const [copied, setCopied] = useState(false)
  // Once Claude in a waiting window registers its session (someone answered
  // what it asked), with whether Remote Control has connected yet.
  const [started, setStarted] = useState<{ remoteControl: boolean } | null>(null)
  const waiting = launch.attention !== null && started === null
  const command = sshAttach(host, launch.attachCommand)
  const title = waiting
    ? launch.alreadyRunning
      ? 'Waiting for you'
      : 'Started, and waiting'
    : launch.alreadyRunning
      ? 'Already running'
      : 'Session started'
  const kind = launch.attention?.kind

  function handleScreen(screen: WindowScreen) {
    if (screen.sessionId === null || started?.remoteControl === screen.remoteControl) {
      return
    }
    setStarted({ remoteControl: screen.remoteControl })
    // It's running now (and maybe on Remote Control): show it in the lists.
    void queryClient.invalidateQueries({ queryKey: queryKeys.remote.sessions(host.id, account) })
    void queryClient.invalidateQueries({ queryKey: queryKeys.remote.accounts(host.id) })
  }

  return (
    <Dialog
      open
      title={title}
      description={`In tmux window ${launch.window.windowId} of session "${launch.window.session}" on ${host.label}.`}
      onClose={onClose}
      onSubmit={waiting ? undefined : onClose}
      closeOnOutsideClick={!launch.attention}
      className={launch.attention ? 'w-[min(680px,calc(100%-64px))]' : undefined}
      foot={
        waiting ? (
          <>
            <Button variant="ghost" size="sm" onClick={onClose}>
              Leave it waiting
            </Button>
            <Button variant="primary" size="sm" disabled>
              Waiting for Claude…
            </Button>
          </>
        ) : (
          <Button variant="primary" size="sm" trailingKbd={<Kbd variant="onOrange">⏎</Kbd>} onClick={onClose}>
            Done
          </Button>
        )
      }
    >
      <div className="space-y-3 text-body text-ink-soft">
        {launch.attention ? (
          <>
            {started ? (
              <p role="status" className="flex items-center gap-2">
                <StatusDot tone="success" />
                {started.remoteControl
                  ? "Claude is running, with Remote Control connected: it's in the Claude app on your other devices."
                  : 'Claude is running. Remote Control is connecting…'}
              </p>
            ) : kind === 'trustPermissions' ? (
              <>
                <p>
                  Claude is asking whether to trust the folder, and trusting it would also allow what the folder's
                  settings pre-approve, so it wasn't answered for you:
                </p>
                <p className="rounded-md border border-border-soft bg-white/40 px-2.5 py-2 font-mono text-[11.5px] text-muted-strong dark:bg-white/[0.03]">
                  {launch.attention.text}
                </p>
                <p>
                  If that's fine, choose "Yes, I trust this folder" below (↓, then ⏎). Remote Control connects then.
                </p>
              </>
            ) : kind === 'trustPrompt' ? (
              <p>
                Claude is asking whether to trust the folder. Answer it below (↓ then ⏎ trusts it); Remote Control
                connects then.
              </p>
            ) : (
              <p>Claude hasn't finished starting. Here's its window: answer whatever it's asking.</p>
            )}
            <RemoteWindow host={host} account={account} windowId={launch.window.windowId} onScreen={handleScreen} />
          </>
        ) : launch.alreadyRunning ? (
          <p>It was open there already, so nothing new was started.</p>
        ) : (
          <p>
            Remote Control is on
            {launch.remoteControlName ? ` as "${launch.remoteControlName}"` : ', named after its folder'}, so it's in
            the Claude app on your other devices.
          </p>
        )}
        {command === null ? (
          <p>The host described its window in a way this Mac won't run, so it can't be opened from here.</p>
        ) : (
          <div className="flex items-center gap-2">
            <code
              className="min-w-0 flex-1 truncate rounded-md border border-border-soft px-2 py-1 font-mono text-[11.5px] text-muted-strong"
              title={command}
            >
              {command}
            </code>
            <Button
              variant="secondary"
              size="sm"
              leadingIcon={<Copy className="h-3.5 w-3.5" />}
              onClick={async () => {
                await copyToClipboard(command)
                setCopied(true)
              }}
            >
              {copied ? 'Copied' : 'Copy'}
            </Button>
            <Button
              variant="secondary"
              size="sm"
              leadingIcon={<SquareTerminal className="h-3.5 w-3.5" />}
              onClick={() =>
                remoteOpenInTerminal({ hostId: host.id, attachCommand: launch.attachCommand }).catch((caught) =>
                  toast.error('Could not open Terminal.', sessionErrorMessage(caught)),
                )
              }
            >
              Terminal
            </Button>
          </div>
        )}
      </div>
    </Dialog>
  )
}
