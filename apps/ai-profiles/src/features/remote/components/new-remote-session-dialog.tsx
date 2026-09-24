import type { RemoteHost, RemoteLaunch } from '@/lib/types'

import { useEffect, useState } from 'react'

import { ChevronLeft, Folder } from 'lucide-react'

import { Button, Dialog, Kbd } from '@/design'
import { Input } from '@/design/ui/input'
import { sessionErrorMessage } from '@/features/profiles/components/session-error-message'
import { shortenHomePath } from '@/features/profiles/components/shorten-home-path'

import { useNewRemoteSession, useRemoteDirs } from '../api/use-remote'

type Props = {
  open: boolean
  host: RemoteHost
  account: string
  /** Folders the account's sessions worked in, most recent first. */
  recentFolders: Array<string>
  onClose: () => void
  onStarted: (launch: RemoteLaunch) => void
}

/** The last part of a path: `david` for `/home/marcus/david`. */
export function folderName(path: string): string {
  return path.split('/').filter(Boolean).pop() ?? ''
}

const labelClasses = 'mb-1.5 block font-mono text-[11.5px] font-medium uppercase tracking-[0.08em] text-muted'

/**
 * Starting a Claude session on a remote host: pick a folder there, maybe
 * name the session (its Remote Control name), and it opens in the host's
 * tmux with Remote Control on.
 */
export function NewRemoteSessionDialog({ open, host, account, recentFolders, onClose, onStarted }: Props) {
  // null: the host's home, which the first listing resolves.
  const [path, setPath] = useState<string | null>(null)
  const [name, setName] = useState('')
  // Until someone types a name, the session is named after its folder.
  const [nameTyped, setNameTyped] = useState(false)
  const [trust, setTrust] = useState(true)
  const dirs = useRemoteDirs(host.id, path, open)
  const start = useNewRemoteSession(host.id, account)

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset each time the dialog opens
  useEffect(() => {
    if (open) {
      setPath(null)
      setName('')
      setNameTyped(false)
      setTrust(true)
      start.reset()
    }
  }, [open])

  const listing = dirs.data
  const home = listing?.home
  const folder = listing?.path ?? null
  const ready = folder !== null && !start.isPending

  useEffect(() => {
    if (!nameTyped && folder !== null) {
      setName(folderName(folder))
    }
  }, [folder, nameTyped])

  async function handleStart() {
    if (!ready || folder === null) {
      return
    }
    try {
      const launch = await start.mutateAsync({ cwd: folder, name: name.trim() || undefined, trustFolder: trust })
      onStarted(launch)
      onClose()
    } catch {
      // Shown below.
    }
  }

  return (
    <Dialog
      open={open}
      title="New session"
      description={`${account} on ${host.label}. It opens in tmux with Remote Control on.`}
      onClose={onClose}
      onSubmit={handleStart}
      closeOnOutsideClick={false}
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={start.isPending} onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            size="sm"
            trailingKbd={<Kbd variant="onOrange">⏎</Kbd>}
            disabled={!ready}
            onClick={handleStart}
          >
            {start.isPending ? 'Starting…' : 'Start'}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div>
          <span className={labelClasses}>Folder</span>
          <div className="rounded-[10px] border border-border-soft bg-white/30 dark:bg-white/[0.02]">
            <div className="flex items-center gap-2 border-b border-border-soft px-[11px] py-[7px]">
              <Button
                variant="ghost"
                size="sm"
                aria-label="Up one folder"
                disabled={!listing?.parent}
                onClick={() => listing?.parent && setPath(listing.parent)}
              >
                <ChevronLeft aria-hidden className="h-3.5 w-3.5" />
              </Button>
              <span className="truncate font-mono text-mono text-ink-soft" title={folder ?? undefined}>
                {folder ? shortenHomePath(folder, home) : '…'}
              </span>
            </div>
            <ul aria-label="Subfolders" className="max-h-[180px] overflow-y-auto py-1">
              {dirs.isError ? (
                <li className="px-[13px] py-1.5 text-meta text-red">{sessionErrorMessage(dirs.error)}</li>
              ) : listing && listing.entries.length === 0 ? (
                <li className="px-[13px] py-1.5 text-meta text-muted">No folders inside. Start here, or go up.</li>
              ) : (
                (listing?.entries ?? []).map((entry) => (
                  <li key={entry.path}>
                    <button
                      type="button"
                      className="flex w-full cursor-pointer items-center gap-2 px-[13px] py-1 text-left text-body text-ink-soft hover:bg-white/50 dark:hover:bg-white/[0.04]"
                      onClick={() => setPath(entry.path)}
                    >
                      <Folder aria-hidden className="h-3.5 w-3.5 shrink-0 text-muted" />
                      <span className="truncate">{entry.name}</span>
                    </button>
                  </li>
                ))
              )}
              {listing?.truncated ? (
                <li className="px-[13px] py-1 text-meta text-muted">Only the first 500 are listed.</li>
              ) : null}
            </ul>
          </div>
          {recentFolders.length > 0 ? (
            <div className="mt-2 flex flex-wrap items-center gap-1.5">
              <span className="text-meta text-muted">Recent:</span>
              {recentFolders.slice(0, 5).map((recent) => (
                <button
                  key={recent}
                  type="button"
                  title={recent}
                  className="max-w-[180px] cursor-pointer truncate rounded-[5px] border border-border-soft px-1.5 py-px font-mono text-[11px] text-muted-strong hover:border-border-strong hover:text-ink"
                  onClick={() => setPath(recent)}
                >
                  {shortenHomePath(recent, home)}
                </button>
              ))}
            </div>
          ) : null}
        </div>

        <div>
          <label htmlFor="remote-session-name" className={labelClasses}>
            Name <span className="normal-case tracking-normal text-muted">(optional)</span>
          </label>
          <Input
            id="remote-session-name"
            value={name}
            maxLength={100}
            onChange={(event) => {
              setName(event.target.value)
              setNameTyped(true)
            }}
            placeholder="Named by Remote Control"
            autoComplete="off"
            spellCheck={false}
          />
          <p className="mt-1 text-meta text-muted">
            What Remote Control, the tmux window and this list call it. The folder's name unless you change it.
          </p>
        </div>

        <label className="flex cursor-pointer items-start gap-2 text-body text-ink-soft">
          <input
            type="checkbox"
            checked={trust}
            onChange={(event) => setTrust(event.target.checked)}
            className="mt-[3px] h-4 w-4 cursor-pointer accent-orange"
          />
          <span>
            Trust this folder
            <span className="block text-meta text-muted">
              Answers Claude's "do you trust this folder?" question for you. Remote Control can't connect until it's
              answered.
            </span>
          </span>
        </label>

        {start.isError ? (
          <p role="alert" className="whitespace-pre-line text-meta text-red">
            {sessionErrorMessage(start.error, 'The session could not be started.')}
          </p>
        ) : null}
      </div>
    </Dialog>
  )
}
