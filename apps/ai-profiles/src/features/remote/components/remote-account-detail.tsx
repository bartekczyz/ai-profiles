import type {
  RemoteAccount,
  RemoteArchivedSession,
  RemoteHost,
  RemoteLaunch,
  RemoteSession,
  RemoteTransferReport,
} from '@/lib/types'

import { useState } from 'react'

import {
  Archive,
  ArchiveRestore,
  ArrowRightLeft,
  Copy,
  Laptop,
  LoaderCircle,
  LogIn,
  MessageSquareReply,
  MoreHorizontal,
  Pencil,
  Play,
  Plus,
  RotateCw,
  Server,
  Square,
  SquareTerminal,
  Trash2,
} from 'lucide-react'

import { PaneLayout } from '@/components/pane-layout'
import { Button, cn, Dialog, Kbd, Skeleton, StatusDot, useToast } from '@/design'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/design/ui/dropdown-menu'
import { Input } from '@/design/ui/input'
import { ColorSwatchPicker } from '@/features/profiles/components/color-swatch-picker'
import {
  headerControlClasses,
  ProfileDetailHeader,
  ProfileSwatch,
} from '@/features/profiles/components/profile-detail-header'
import { isValidRemoteProfileName } from '@/features/profiles/components/profile-form-fields'
import { sessionErrorMessage } from '@/features/profiles/components/session-error-message'
import { SessionRowBase, sessionPanelClasses, sessionRowClasses } from '@/features/profiles/components/session-row-base'
import { shortenHomePath } from '@/features/profiles/components/shorten-home-path'
import { isValidHexColor, presetColors } from '@/lib/colors'
import { copyToClipboard, remoteOpenInClaude, remoteOpenInTerminal } from '@/lib/commands'
import { formatBytes } from '@/lib/format-bytes'

import {
  useArchiveRemoteSession,
  useDeleteRemoteArchive,
  useRemoteAccounts,
  useRemoteArchivedSessions,
  useRemoteHostInfo,
  useRemoteHosts,
  useRemoteLogout,
  useRemoteSessions,
  useRenameRemoteProfile,
  useRenameRemoteSession,
  useRestartRemoteSession,
  useRestoreRemoteSession,
  useResumeRemoteSession,
  useSetRemoteProfileColor,
  useStopRemoteSession,
} from '../api/use-remote'
import { DeleteRemoteAccountDialog } from './delete-remote-account-dialog'
import { LaunchResultDialog, sshAttach } from './launch-result-dialog'
import { MoveRemoteSessionDialog } from './move-remote-session-dialog'
import { NewRemoteSessionDialog } from './new-remote-session-dialog'
import { RemoteWindow } from './remote-window'
import { SignInDialog } from './sign-in-dialog'

type Props = {
  hostId: string
  account: string
  /** It was renamed: the sidebar selection follows it. */
  onRenamed?: (name: string) => void
}

/**
 * A profile on a remote host (a Claude account there): whose it is, how long
 * its sign-in lasts, and its sessions, with where each running one is in tmux.
 */
export function RemoteAccountDetail({ hostId, account, onRenamed }: Props) {
  const host = useRemoteHosts().find((candidate) => candidate.id === hostId)
  if (!host) {
    return (
      <PaneLayout header={null}>
        <p className="text-body text-muted">That host is no longer paired.</p>
      </PaneLayout>
    )
  }
  return <Detail host={host} accountName={account} onRenamed={onRenamed} />
}

function Detail({
  host,
  accountName,
  onRenamed,
}: {
  host: RemoteHost
  accountName: string
  onRenamed?: (name: string) => void
}) {
  const accounts = useRemoteAccounts(host.id)
  const info = useRemoteHostInfo(host.id)
  const sessions = useRemoteSessions(host.id, accountName)
  const account = accounts.data?.find((candidate) => candidate.name === accountName)
  const [starting, setStarting] = useState(false)
  // A launch to say more about, and the profile it runs under: a move's
  // resume runs under the profile it moved to, not this one.
  const [result, setResult] = useState<{ launch: RemoteLaunch; account: string } | null>(null)
  const [signingIn, setSigningIn] = useState(false)
  const [signingOut, setSigningOut] = useState(false)
  const [editing, setEditing] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const color = host.profiles?.[accountName]?.color ?? null
  const recentFolders = [
    ...new Set((sessions.data ?? []).map((session) => session.cwd).filter((cwd): cwd is string => cwd !== null)),
  ]

  return (
    <PaneLayout
      header={
        <ProfileDetailHeader
          name={accountName}
          swatch={color ? <ProfileSwatch color={color} /> : <HostSwatch />}
          subline={<Subline host={host} account={account} offline={accounts.isError} />}
          action={
            <button
              type="button"
              className={headerControlClasses}
              disabled={accounts.isError || !account}
              onClick={() => setStarting(true)}
            >
              <Plus aria-hidden className="h-3.5 w-3.5" strokeWidth={1.9} />
              New session
            </button>
          }
          menu={
            account && !accounts.isError ? (
              <ProfileMenu
                account={account}
                onEdit={() => setEditing(true)}
                onSignIn={() => setSigningIn(true)}
                onSignOut={() => setSigningOut(true)}
                onDelete={() => setDeleting(true)}
              />
            ) : undefined
          }
        />
      }
    >
      {account && !account.signedIn && !accounts.isError ? (
        <div className="mb-5 flex items-center justify-between gap-3 rounded-[10px] border border-border-soft px-[13px] py-[10px]">
          <p className="text-body text-ink-soft">
            Signed out. Sessions started here will ask to sign in before they do anything.
          </p>
          <Button
            variant="secondary"
            size="sm"
            leadingIcon={<LogIn className="h-3.5 w-3.5" />}
            onClick={() => setSigningIn(true)}
          >
            Sign in
          </Button>
        </div>
      ) : null}
      {accounts.isSuccess && !account ? (
        <p className="text-body text-muted">
          {host.label} has no profile called {accountName} any more.
        </p>
      ) : (
        <RemoteSessions
          host={host}
          account={accountName}
          email={account?.account?.email ?? null}
          home={info.data?.home}
          onResult={(launch, under) => setResult({ launch, account: under ?? accountName })}
        />
      )}
      <NewRemoteSessionDialog
        open={starting}
        host={host}
        account={accountName}
        recentFolders={recentFolders}
        onClose={() => setStarting(false)}
        onStarted={(launch) => {
          // As with Resume: only when there's more to say than that it
          // started, which the list shows.
          if (launch.attention || launch.alreadyRunning) {
            setResult({ launch, account: accountName })
          }
        }}
      />
      <SignInDialog open={signingIn} host={host} account={accountName} onClose={() => setSigningIn(false)} />
      {account ? (
        <SignOutDialog open={signingOut} host={host} account={account} onClose={() => setSigningOut(false)} />
      ) : null}
      {editing ? (
        <EditRemoteProfileDialog
          open
          host={host}
          account={account ?? null}
          name={accountName}
          taken={(accounts.data ?? []).map((candidate) => candidate.name)}
          color={color}
          onClose={() => setEditing(false)}
          onRenamed={(name) => onRenamed?.(name)}
        />
      ) : null}
      {account ? (
        <DeleteRemoteAccountDialog
          open={deleting}
          host={host}
          account={account}
          onClose={() => setDeleting(false)}
          onDeleted={() => undefined}
        />
      ) : null}
      {result ? (
        <LaunchResultDialog
          host={host}
          account={result.account}
          launch={result.launch}
          onClose={() => setResult(null)}
        />
      ) : null}
    </PaneLayout>
  )
}

const menuTriggerClasses = 'w-7 px-0 text-muted hover:not-disabled:text-ink'

/**
 * Rare things done to the profile: its color, signing it in or out, and
 * deleting it.
 */
function ProfileMenu({
  account,
  onEdit,
  onSignIn,
  onSignOut,
  onDelete,
}: {
  account: RemoteAccount
  onEdit: () => void
  onSignIn: () => void
  onSignOut: () => void
  onDelete: () => void
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button type="button" aria-label="More actions" className={cn(headerControlClasses, menuTriggerClasses)}>
          <MoreHorizontal aria-hidden className="h-4 w-4" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-52">
        <DropdownMenuItem className="px-2 py-1.5 text-[12px]" onSelect={onEdit}>
          Edit profile…
        </DropdownMenuItem>
        <DropdownMenuItem className="px-2 py-1.5 text-[12px]" onSelect={account.signedIn ? onSignOut : onSignIn}>
          {account.signedIn ? 'Sign out…' : 'Sign in…'}
        </DropdownMenuItem>
        {account.isDefault ? null : (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" className="px-2 py-1.5 text-[12px]" onSelect={onDelete}>
              Delete profile…
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

/** How long a sign-in has left, in words: "27 days left", "5 hours left". */
export function signInLeft(until: string | null, now: number = Date.now()): string | null {
  if (until === null) {
    return null
  }
  const left = new Date(until).getTime() - now
  if (Number.isNaN(left)) {
    return null
  }
  const hour = 3_600_000
  const day = 24 * hour
  if (left <= 0) {
    return 'expired'
  }
  if (left >= 2 * day) {
    return `${Math.floor(left / day)} days left`
  }
  if (left >= day) {
    return '1 day left'
  }
  if (left >= 2 * hour) {
    return `${Math.floor(left / hour)} hours left`
  }
  return 'less than 2 hours left'
}

/**
 * Signing a profile out: `claude auth logout` on the host. Its running
 * sessions would go on until their token next renews and then fail, so they
 * are stopped first, which this says before it's done.
 */
function SignOutDialog({
  open,
  host,
  account,
  onClose,
}: {
  open: boolean
  host: RemoteHost
  account: RemoteAccount
  onClose: () => void
}) {
  const logout = useRemoteLogout(host.id, account.name)
  const toast = useToast()
  const running = account.runningSessions
  async function handleSignOut() {
    try {
      const stopped = await logout.mutateAsync(running > 0)
      toast.success(
        `Signed out ${account.name}`,
        stopped > 0
          ? `${stopped} running ${stopped === 1 ? 'session was' : 'sessions were'} stopped first.`
          : undefined,
      )
      onClose()
    } catch (caught) {
      toast.error('Could not sign it out.', sessionErrorMessage(caught))
    }
  }
  return (
    <Dialog
      open={open}
      title={`Sign out ${account.name}?`}
      description={account.account?.email ?? `On ${host.label}.`}
      onClose={onClose}
      onSubmit={handleSignOut}
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={logout.isPending} onClick={onClose}>
            Cancel
          </Button>
          <Button variant="danger" size="sm" disabled={logout.isPending} onClick={handleSignOut}>
            {logout.isPending ? 'Signing out…' : running > 0 ? 'Stop sessions and sign out' : 'Sign out'}
          </Button>
        </>
      }
    >
      <p className="text-body text-ink-soft">
        {running > 0
          ? `${running} ${running === 1 ? 'session is' : 'sessions are'} running. ${running === 1 ? 'It stops' : 'They stop'} first: signed out, ${running === 1 ? 'it' : 'they'} would fail as soon as the token renews. `
          : ''}
        Its sessions are kept. Sign in again to use them.
      </p>
    </Dialog>
  )
}

/**
 * A remote profile's name and color. The name is its account folder on the
 * server, so it must be one the server takes and no other profile there has
 * (ignoring case); running sessions have that folder open, so renaming stops
 * them first, which this says.
 */
function EditRemoteProfileDialog({
  open,
  host,
  account,
  name,
  taken,
  color,
  onClose,
  onRenamed,
}: {
  open: boolean
  host: RemoteHost
  account: RemoteAccount | null
  name: string
  /** Every profile name on the host, this one included. */
  taken: Array<string>
  color: string | null
  onClose: () => void
  onRenamed: (name: string) => void
}) {
  const [picked, setPicked] = useState(color ?? presetColors[0])
  const [newName, setNewName] = useState(name)
  const rename = useRenameRemoteProfile(host.id)
  const save = useSetRemoteProfileColor(host.id)
  const toast = useToast()
  const trimmed = newName.trim()
  const renaming = trimmed !== name
  const clash = renaming && taken.some((other) => other !== name && other.toLowerCase() === trimmed.toLowerCase())
  const invalid = renaming && !isValidRemoteProfileName(trimmed)
  const running = account?.runningSessions ?? 0
  const fixedName = account?.isDefault ?? false
  const busy = rename.isPending || save.isPending
  const ready = !busy && !clash && !invalid && isValidHexColor(picked)

  async function handleSave() {
    if (!ready) {
      return
    }
    try {
      let current = name
      if (renaming) {
        current = (await rename.mutateAsync({ account: name, newName: trimmed, stopRunning: running > 0 })).name
      }
      if (picked !== color) {
        await save.mutateAsync({ account: current, color: picked })
      }
      onClose()
      if (current !== name) {
        onRenamed(current)
      }
    } catch (caught) {
      toast.error('Could not save it.', sessionErrorMessage(caught))
    }
  }

  return (
    <Dialog
      open={open}
      title="Edit profile"
      description={`On ${host.label}.`}
      onClose={onClose}
      onSubmit={handleSave}
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={busy} onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            size="sm"
            trailingKbd={<Kbd variant="onOrange">⏎</Kbd>}
            disabled={!ready}
            onClick={handleSave}
          >
            {busy ? 'Saving…' : renaming && running > 0 ? 'Stop sessions and save' : 'Save'}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div>
          <label htmlFor="remote-profile-name" className="mb-1.5 block text-meta text-ink-soft">
            Name
          </label>
          <Input
            id="remote-profile-name"
            autoFocus
            value={newName}
            disabled={fixedName}
            onChange={(event) => setNewName(event.target.value)}
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            spellCheck={false}
          />
          <span className={cn('mt-1.5 block font-mono text-mono', clash || invalid ? 'text-red' : 'text-muted-strong')}>
            {fixedName
              ? 'The default profile is ~/.claude, and keeps its name.'
              : clash
                ? `${host.label} already has a profile called ${trimmed}.`
                : invalid
                  ? 'Letters, digits, - and _, starting with a letter or digit. "default" is taken.'
                  : `On ${host.label}: ~/.claude-accounts/${trimmed || name}`}
          </span>
          {renaming && running > 0 && !clash && !invalid ? (
            <span className="mt-1 block text-meta text-amber">
              {running} running {running === 1 ? 'session has' : 'sessions have'} its folder open, and{' '}
              {running === 1 ? 'stops' : 'stop'} first. Resume {running === 1 ? 'it' : 'them'} from Previous.
            </span>
          ) : null}
        </div>
        <div>
          <span className="mb-1.5 block text-meta text-ink-soft">Color</span>
          <ColorSwatchPicker value={picked} onChange={setPicked} />
        </div>
      </div>
    </Dialog>
  )
}

function HostSwatch() {
  return (
    <div
      aria-hidden
      className="grid h-11 w-11 shrink-0 place-items-center rounded-xl bg-white text-ink-soft shadow-[inset_0_0_0_1px_rgba(0,0,0,0.06),0_2px_6px_-3px_rgba(0,0,0,0.18)] dark:bg-cream-2"
    >
      <Server className="h-5 w-5" strokeWidth={1.6} />
    </div>
  )
}

function Subline({
  host,
  account,
  offline,
}: {
  host: RemoteHost
  account: RemoteAccount | undefined
  offline: boolean
}) {
  const who = account?.account
  const identity = account?.signedIn ? (who?.email ?? who?.name ?? null) : null
  const plan = account?.signedIn ? (who?.plan ?? null) : null
  const left = account?.signedIn ? signInLeft(account.signedInUntil) : null
  const until = account?.signedInUntil ? new Date(account.signedInUntil).toLocaleString() : null
  const separator = <span className="mx-2 text-border">·</span>
  return (
    <>
      <span>{host.label}</span>
      {offline ? (
        <>
          {separator}
          <span className="text-red">offline</span>
        </>
      ) : account && !account.signedIn ? (
        <>
          {separator}
          <span className="text-amber">Signed out</span>
        </>
      ) : account ? (
        <>
          {identity ? (
            <>
              {separator}
              <span title={account.configDir}>{identity}</span>
            </>
          ) : null}
          {plan ? (
            <>
              {separator}
              <span>{plan}</span>
            </>
          ) : null}
          {left ? (
            <>
              {separator}
              <span className="text-muted-strong" title={until ? `Signed in until ${until}` : undefined}>
                {left}
              </span>
            </>
          ) : null}
        </>
      ) : null}
    </>
  )
}

/** Previous sessions shown before "Show all". */
const collapsedCount = 8

type RemoteSessionsProps = {
  host: RemoteHost
  account: string
  /** Whose account it is: Open in Claude goes to the app here signed in as it. */
  email: string | null
  home?: string
  /**
   * A start or resume worth showing more about than a toast, and the profile
   * it runs under when that isn't this one (after a move).
   */
  onResult: (launch: RemoteLaunch, account?: string) => void
}

/** What a row is busy with, while the host does it. */
type Busy = { id: string; action: 'resume' | 'restart' | 'stop' | 'archive' | 'restore' | 'delete' }

/** Something held until the user confirms it. */
type Pending =
  | { kind: 'stop'; session: RemoteSession }
  | { kind: 'restartAll'; sessions: Array<RemoteSession> }
  | { kind: 'archive'; session: RemoteSession }

/**
 * The profile's sessions: the running ones, which can be stopped or
 * restarted (restarting is how a session picks up an upgraded `claude`), and
 * the previous ones, which can be resumed.
 */
function RemoteSessions({ host, account, email, home, onResult }: RemoteSessionsProps) {
  const sessions = useRemoteSessions(host.id, account)
  const resume = useResumeRemoteSession(host.id, account)
  const restart = useRestartRemoteSession(host.id, account)
  const stop = useStopRemoteSession(host.id, account)
  const archive = useArchiveRemoteSession(host.id, account)
  const restore = useRestoreRemoteSession(host.id, account)
  const deleteArchive = useDeleteRemoteArchive(host.id, account)
  const [deleting, setDeleting] = useState<RemoteArchivedSession | null>(null)
  const archivedSessions = useRemoteArchivedSessions(host.id, account)
  const [showArchived, setShowArchived] = useState(false)
  const [moving, setMoving] = useState<RemoteSession | null>(null)
  const [renaming, setRenaming] = useState<RemoteSession | null>(null)
  const [viewing, setViewing] = useState<RemoteSession | null>(null)
  const hostInfo = useRemoteHostInfo(host.id)
  const toast = useToast()
  const [busy, setBusy] = useState<Busy | null>(null)
  const [pending, setPending] = useState<Pending | null>(null)
  const [restartingAll, setRestartingAll] = useState<{ done: number; total: number } | null>(null)

  async function handleResume(session: RemoteSession) {
    setBusy({ id: session.id, action: 'resume' })
    try {
      const launch = await resume.mutateAsync(session.id)
      if (launch.attention || launch.alreadyRunning) {
        onResult(launch)
      } else {
        toast.success(`Resumed on ${host.label}`, `In tmux window ${launch.window.windowId}, with Remote Control on.`)
      }
    } catch (caught) {
      toast.error('Could not resume it.', sessionErrorMessage(caught))
    } finally {
      setBusy(null)
    }
  }

  async function handleArchive(session: RemoteSession) {
    setBusy({ id: session.id, action: 'archive' })
    try {
      await archive.mutateAsync(session.id)
      toast.success('Archived', session.title ?? session.lastPrompt ?? session.id)
    } catch (caught) {
      toast.error('Could not archive it.', sessionErrorMessage(caught))
    } finally {
      setBusy(null)
    }
  }

  async function handleRestore(archived: RemoteArchivedSession) {
    setBusy({ id: archived.id, action: 'restore' })
    try {
      await restore.mutateAsync({ sessionId: archived.id, archive: archived.archive })
      toast.success('Restored', archived.title ?? archived.id)
    } catch (caught) {
      toast.error('Could not restore it.', sessionErrorMessage(caught))
    } finally {
      setBusy(null)
    }
  }

  async function handleDeleteArchive(archived: RemoteArchivedSession) {
    setDeleting(null)
    setBusy({ id: archived.id, action: 'delete' })
    try {
      const freed = await deleteArchive.mutateAsync({ sessionId: archived.id, archive: archived.archive })
      toast.success(`Deleted, freeing ${formatBytes(freed)}`, archived.title ?? archived.id)
    } catch (caught) {
      toast.error('Could not delete it.', sessionErrorMessage(caught))
    } finally {
      setBusy(null)
    }
  }

  function handleMoved(report: RemoteTransferReport, to: string) {
    if (report.deleteError) {
      toast.error(`Moved to ${to}. The session on ${account} was kept.`, report.deleteError)
    }
    if (report.resumeError) {
      toast.error(`Moved to ${to}, but it didn't resume.`, report.resumeError)
    } else if (report.launch?.attention) {
      onResult(report.launch, to)
    } else {
      const details = [
        report.launch ? `Resumed in tmux window ${report.launch.window.windowId}.` : null,
        report.freedBytes !== null ? `Deleted the copy in ${account}.` : null,
        report.memory.length > 0
          ? `Project memory: ${report.memory.length} ${report.memory.length === 1 ? 'note' : 'notes'} brought over.`
          : null,
      ]
        .filter(Boolean)
        .join(' ')
      toast.success(report.changed ? `Moved to ${to}` : `${to} was already up to date`, details || undefined)
    }
  }

  async function handleRestart(session: RemoteSession) {
    setBusy({ id: session.id, action: 'restart' })
    try {
      const launch = await restart.mutateAsync(session.id)
      if (launch.attention) {
        onResult(launch)
      } else {
        toast.success(
          `Restarted on ${host.label}`,
          `In tmux window ${launch.window.windowId}, on the host's current claude.`,
        )
      }
    } catch (caught) {
      toast.error('Could not restart it.', sessionErrorMessage(caught))
    } finally {
      setBusy(null)
    }
  }

  async function handleStop(session: RemoteSession) {
    setBusy({ id: session.id, action: 'stop' })
    try {
      await stop.mutateAsync(session.id)
      toast.success('Stopped', session.title ?? session.lastPrompt ?? session.id)
    } catch (caught) {
      toast.error('Could not stop it.', sessionErrorMessage(caught))
    } finally {
      setBusy(null)
    }
  }

  /** One at a time: each is a stop and a start on the host. */
  async function handleRestartAll(targets: Array<RemoteSession>) {
    const failed: Array<string> = []
    let waiting = 0
    setRestartingAll({ done: 0, total: targets.length })
    for (const [index, session] of targets.entries()) {
      try {
        const launch = await restart.mutateAsync(session.id)
        if (launch.attention) {
          waiting += 1
        }
      } catch (caught) {
        failed.push(`${session.title ?? session.id}: ${sessionErrorMessage(caught)}`)
      }
      setRestartingAll({ done: index + 1, total: targets.length })
    }
    setRestartingAll(null)
    const restarted = targets.length - failed.length
    if (failed.length > 0) {
      toast.error(`Restarted ${restarted} of ${targets.length}.`, failed.join('\n'))
    } else {
      toast.success(
        `Restarted ${restarted} ${restarted === 1 ? 'session' : 'sessions'} on ${host.label}`,
        waiting > 0 ? `${waiting} ${waiting === 1 ? 'is' : 'are'} waiting for you in its window.` : undefined,
      )
    }
  }

  const [expanded, setExpanded] = useState(false)
  const list = sessions.data ?? []
  const running = list.filter((session) => session.running)
  // Sessions on an older claude than the host now has installed.
  const updating = running.filter((session) => session.updatePending).length
  const previous = list.filter((session) => !session.running)
  const visiblePrevious = expanded ? previous : previous.slice(0, collapsedCount)
  const locked = busy !== null || restartingAll !== null

  const row = (session: RemoteSession) => (
    <RemoteSessionRow
      key={session.id}
      onView={() => setViewing(session)}
      host={host}
      session={session}
      email={email}
      home={home}
      busy={busy?.id === session.id ? busy.action : restartingAll ? 'restart' : null}
      disabled={locked}
      onResume={() => void handleResume(session)}
      onRestart={() => void handleRestart(session)}
      onStop={() => setPending({ kind: 'stop', session })}
      onMove={() => setMoving(session)}
      onRename={() => setRenaming(session)}
      onArchive={() => setPending({ kind: 'archive', session })}
    />
  )

  return (
    <section aria-label="Sessions" className="mb-6">
      {sessions.isLoading ? (
        <div className={sessionPanelClasses}>
          <div className={sessionRowClasses}>
            <Skeleton shape="text" className="w-2/3" />
          </div>
        </div>
      ) : sessions.isError ? (
        <div className={sessionPanelClasses}>
          <div className="flex items-center justify-between gap-3 px-[13px] py-[10px]">
            <p role="alert" className="text-meta text-red">
              {sessionErrorMessage(sessions.error, `Could not read ${host.label}'s sessions.`)}
            </p>
            <Button
              variant="ghost"
              size="sm"
              leadingIcon={<RotateCw className="h-3.5 w-3.5" />}
              onClick={() => void sessions.refetch()}
            >
              Retry
            </Button>
          </div>
        </div>
      ) : list.length === 0 ? (
        <>
          <SessionsHeading title="Sessions" />
          <div className={sessionPanelClasses}>
            <p className="px-[13px] py-[10px] text-meta text-muted">No sessions yet.</p>
          </div>
        </>
      ) : (
        <>
          {running.length > 0 ? (
            <div className="mb-4">
              <SessionsHeading title="Running" count={running.length}>
                {running.length > 1 || updating > 0 ? (
                  <Button
                    variant="ghost"
                    size="sm"
                    leadingIcon={<RotateCw className="h-3.5 w-3.5" />}
                    disabled={locked}
                    title={
                      updating > 0
                        ? `Stop every running session and start it again on the host's current claude: ${updating} of them run an older one`
                        : "Stop every running session and start it again on the host's current claude"
                    }
                    className={updating > 0 ? 'text-amber hover:text-amber' : undefined}
                    onClick={() => setPending({ kind: 'restartAll', sessions: running })}
                  >
                    {restartingAll
                      ? `Restarting ${restartingAll.done + 1} of ${restartingAll.total}…`
                      : updating > 0
                        ? `Restart all · ${updating} to update`
                        : 'Restart all'}
                  </Button>
                ) : null}
              </SessionsHeading>
              <div className={sessionPanelClasses}>
                <ul aria-label="Running sessions">{running.map(row)}</ul>
              </div>
            </div>
          ) : null}
          {previous.length > 0 ? (
            <div>
              <div className={sessionPanelClasses}>
                <ul aria-label="Previous sessions">{visiblePrevious.map(row)}</ul>
              </div>
            </div>
          ) : null}
        </>
      )}
      {previous.length > collapsedCount || (archivedSessions.data?.length ?? 0) > 0 ? (
        // As on a local profile: Show all on the left, Archived on the right.
        <div className="mt-1.5 flex items-baseline justify-between px-0.5">
          {previous.length > collapsedCount ? (
            <button
              type="button"
              className="cursor-pointer text-meta text-muted-strong hover:text-ink"
              onClick={() => setExpanded((value) => !value)}
            >
              {expanded ? 'Show fewer' : `Show all ${previous.length}`}
            </button>
          ) : (
            <span />
          )}
          {(archivedSessions.data?.length ?? 0) > 0 ? (
            <button
              type="button"
              aria-expanded={showArchived}
              className="cursor-pointer text-meta text-muted-strong hover:text-ink"
              onClick={() => setShowArchived((value) => !value)}
            >
              {showArchived
                ? 'Hide archived'
                : `Archived ${archivedSessions.data?.length} · ${formatBytes((archivedSessions.data ?? []).reduce((total, archived) => total + archived.sizeBytes, 0))}`}
            </button>
          ) : null}
        </div>
      ) : null}
      {(archivedSessions.data?.length ?? 0) > 0 ? (
        <div>
          {showArchived ? (
            <div className={cn(sessionPanelClasses, 'mt-2')}>
              <ul aria-label="Archived sessions">
                {archivedSessions.data?.map((archived) => (
                  <SessionRowBase
                    key={`${archived.id}/${archived.archive}`}
                    title={archived.title ?? archived.id}
                    badges={
                      <span className="shrink-0 text-meta text-muted-strong">{formatBytes(archived.sizeBytes)}</span>
                    }
                    folder={archived.cwd}
                    home={home}
                    at={stampToIso(archived.stamp)}
                    actions={
                      <>
                        <Button
                          variant="ghost"
                          size="sm"
                          leadingIcon={<ArchiveRestore className="h-3.5 w-3.5" />}
                          disabled={locked}
                          className={rowActionClasses}
                          onClick={() => void handleRestore(archived)}
                        >
                          {busy?.id === archived.id && busy.action === 'restore' ? 'Restoring…' : 'Restore'}
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          aria-label="Delete"
                          title={`Delete this archive for good, freeing ${formatBytes(archived.sizeBytes)}`}
                          disabled={locked}
                          className={rowActionClasses}
                          onClick={() => setDeleting(archived)}
                        >
                          <Trash2 aria-hidden className="h-3.5 w-3.5" />
                        </Button>
                      </>
                    }
                  />
                ))}
              </ul>
            </div>
          ) : null}
        </div>
      ) : null}
      {deleting ? (
        <Dialog
          open
          title="Delete this archive?"
          description={deleting.title ?? deleting.id}
          onClose={() => setDeleting(null)}
          onSubmit={() => void handleDeleteArchive(deleting)}
          foot={
            <>
              <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} onClick={() => setDeleting(null)}>
                Cancel
              </Button>
              <Button
                variant="danger"
                size="sm"
                trailingKbd={<Kbd>⏎</Kbd>}
                onClick={() => void handleDeleteArchive(deleting)}
              >
                Delete, freeing {formatBytes(deleting.sizeBytes)}
              </Button>
            </>
          }
        >
          <p className="text-body text-ink-soft">
            The archived transcript goes for good, and the session can't be restored. {host.label} gets back{' '}
            {formatBytes(deleting.sizeBytes)}.
          </p>
        </Dialog>
      ) : null}
      {viewing ? (
        <SessionWindowDialog
          host={host}
          account={account}
          session={viewing}
          // Only the windows the server opened can be shown and typed into.
          ours={viewing.window?.session === hostInfo.data?.tmux?.session}
          onClose={() => setViewing(null)}
        />
      ) : null}
      {renaming ? (
        <RenameRemoteSessionDialog host={host} account={account} session={renaming} onClose={() => setRenaming(null)} />
      ) : null}
      {moving ? (
        <MoveRemoteSessionDialog
          open
          host={host}
          account={account}
          session={moving}
          onClose={() => setMoving(null)}
          onMoved={handleMoved}
        />
      ) : null}
      <ConfirmEndDialog
        pending={pending}
        hostLabel={host.label}
        onClose={() => setPending(null)}
        onConfirm={(confirmed) => {
          setPending(null)
          if (confirmed.kind === 'stop') {
            void handleStop(confirmed.session)
          } else if (confirmed.kind === 'archive') {
            void handleArchive(confirmed.session)
          } else {
            void handleRestartAll(confirmed.sessions)
          }
        }}
      />
    </section>
  )
}

function SessionsHeading({ title, count, children }: { title: string; count?: number; children?: React.ReactNode }) {
  return (
    <div className="mb-2 flex min-h-7 items-center justify-between px-0.5">
      <h2 className="text-meta font-medium text-ink-soft">
        {title}
        {count !== undefined ? <span className="ml-1.5 font-normal text-muted">{count}</span> : null}
      </h2>
      {children}
    </div>
  )
}

/**
 * Asks before ending running sessions: whatever Claude is in the middle of
 * stops with them. Their conversations are kept, and can be resumed.
 */
function ConfirmEndDialog({
  pending,
  hostLabel,
  onClose,
  onConfirm,
}: {
  pending: Pending | null
  hostLabel: string
  onClose: () => void
  onConfirm: (pending: Pending) => void
}) {
  const text = pending === null ? null : confirmText(pending, hostLabel)
  return (
    <Dialog
      open={pending !== null}
      title={text?.title ?? ''}
      description={text?.description}
      onClose={onClose}
      onSubmit={() => pending && onConfirm(pending)}
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant={text?.danger ? 'danger' : 'primary'}
            size="sm"
            trailingKbd={<Kbd variant={text?.danger ? undefined : 'onOrange'}>⏎</Kbd>}
            onClick={() => pending && onConfirm(pending)}
          >
            {text?.confirm}
          </Button>
        </>
      }
    >
      <p className="text-body text-ink-soft">{text?.body}</p>
    </Dialog>
  )
}

/** What the confirmation says for each thing held for it. */
function confirmText(pending: Pending, hostLabel: string) {
  const name = (session: RemoteSession) => session.title ?? session.lastPrompt ?? session.id
  switch (pending.kind) {
    case 'stop':
      return {
        title: 'Stop this session?',
        description: name(pending.session),
        body: pending.session.empty
          ? "Claude ends. Nothing has been said in it yet, so there's nothing to resume: it leaves the list."
          : 'Claude ends, and anything it is in the middle of stops with it. The conversation is kept: resume it from Previous.',
        confirm: 'Stop',
        danger: true,
      }
    case 'archive':
      return {
        title: 'Archive this session?',
        description: name(pending.session),
        body: "Its transcript moves to session-transfer-backups, so it's no longer listed, here or in Claude's own /resume. Restore it from Archived.",
        confirm: 'Archive',
        danger: false,
      }
    case 'restartAll':
      return {
        title: `Restart ${pending.sessions.length} running sessions?`,
        description: `On ${hostLabel}.`,
        body: "Each one stops and starts again on the host's current claude, one after another, keeping its conversation. Anything they are in the middle of stops.",
        confirm: 'Restart all',
        danger: false,
      }
  }
}

/** A host's `%Y%m%d-%H%M%S` stamp as a time, read as this Mac's local time. */
function stampToIso(stamp: string): string {
  const match = /^(\d{4})(\d{2})(\d{2})-(\d{2})(\d{2})(\d{2})$/.exec(stamp)
  if (!match) {
    return new Date().toISOString()
  }
  const [year, month, day, hour, minute, second] = match.slice(1).map(Number)
  return new Date(year, month - 1, day, hour, minute, second).toISOString()
}

/**
 * What to paste in a terminal to be in a session's tmux window: attach to
 * its tmux session over ssh, then switch to its window.
 */
export function attachCommand(host: RemoteHost, session: RemoteSession): string | null {
  const tmux = tmuxAttach(session)
  return tmux ? sshAttach(host, tmux) : null
}

/** The part that runs on the host: attach to the tmux session, then pick the window. */
function tmuxAttach(session: RemoteSession): string | null {
  return session.window
    ? `tmux attach -t ${session.window.session} \\; select-window -t ${session.window.windowId}`
    : null
}

/**
 * Open session's tmux window in Terminal, attached over ssh. The command is
 * copied as well, for a terminal other than Terminal, or another Mac.
 */
async function openInTerminal(host: RemoteHost, session: RemoteSession, toast: ReturnType<typeof useToast>) {
  const attach = attachCommand(host, session)
  const tmux = tmuxAttach(session)
  if (!attach || !tmux) {
    return
  }
  await copyToClipboard(attach)
  try {
    const hint = await remoteOpenInTerminal({ hostId: host.id, attachCommand: tmux })
    if (hint) {
      toast.info('Opening Terminal', hint)
    }
  } catch (caught) {
    toast.error('Could not open Terminal.', `${sessionErrorMessage(caught)} The attach command is on the clipboard.`)
  }
}

/**
 * A running session's tmux window, live, in ai-profiles: what it shows, and
 * what's typed into it goes there. A window the server didn't open (one
 * started in a tmux session of the user's own) can't be shown: the server
 * only reads and types into windows it opened, where it knows keys reach
 * Claude and not a shell. Terminal, or the attach command, reach either.
 */
function SessionWindowDialog({
  host,
  account,
  session,
  ours,
  onClose,
}: {
  host: RemoteHost
  account: string
  session: RemoteSession
  /** The server opened the window, so it can be shown here. */
  ours: boolean
  onClose: () => void
}) {
  const toast = useToast()
  const [copied, setCopied] = useState(false)
  const attach = attachCommand(host, session)
  if (!session.window) {
    return null
  }
  return (
    <Dialog
      open
      title={session.title ?? session.lastPrompt ?? session.id}
      description={`tmux window ${session.window.windowId} of session "${session.window.session}" on ${host.label}.`}
      className="w-[min(920px,calc(100%-64px))]"
      closeOnOutsideClick={false}
      onClose={onClose}
      foot={
        <>
          {attach ? (
            <Button
              variant="ghost"
              size="sm"
              leadingIcon={<Copy className="h-3.5 w-3.5" />}
              title={attach}
              onClick={async () => {
                await copyToClipboard(attach)
                setCopied(true)
              }}
            >
              {copied ? 'Copied' : 'Copy attach command'}
            </Button>
          ) : null}
          <Button
            variant="ghost"
            size="sm"
            leadingIcon={<SquareTerminal className="h-3.5 w-3.5" />}
            onClick={() => void openInTerminal(host, session, toast)}
          >
            Open in Terminal
          </Button>
          <Button variant="primary" size="sm" onClick={onClose}>
            Close
          </Button>
        </>
      }
    >
      {ours ? (
        <RemoteWindow host={host} account={account} windowId={session.window.windowId} />
      ) : (
        <div
          role="alert"
          className="rounded-md border border-border-soft bg-white/40 px-3 py-6 text-center text-body text-ink-soft dark:bg-white/[0.03]"
        >
          <p>This window can't be shown here.</p>
          <p className="mt-1 text-meta text-muted">
            It's in tmux session "{session.window.session}", which ai-profiles didn't start. ai-profiles only shows and
            types into windows it opened, where it knows the keys reach Claude and not a shell. Open it in Terminal, or
            copy the attach command.
          </p>
        </div>
      )}
    </Dialog>
  )
}

/**
 * A row's actions are held while another is under way (one at a time), but
 * without the not-allowed cursor: it would show under the pointer the moment
 * one is clicked.
 */
const rowActionClasses = 'disabled:cursor-default'

/** Stop, spinning while the host ends the session: Claude takes a few seconds to go. */
function StopButton({ stopping, disabled, onStop }: { stopping: boolean; disabled: boolean; onStop: () => void }) {
  return (
    <Button
      variant="ghost"
      size="sm"
      aria-label={stopping ? 'Stopping' : 'Stop'}
      title={stopping ? 'Stopping…' : 'Stop'}
      disabled={disabled}
      className={rowActionClasses}
      onClick={onStop}
    >
      {stopping ? (
        <LoaderCircle aria-hidden className="h-3.5 w-3.5 animate-spin" />
      ) : (
        <Square aria-hidden className="h-3.5 w-3.5" />
      )}
    </Button>
  )
}

type RemoteSessionRowProps = {
  host: RemoteHost
  session: RemoteSession
  email: string | null
  home?: string
  /** What the host is doing to this session, if anything. */
  busy: Busy['action'] | null
  /** Another session is being changed: one at a time. */
  disabled: boolean
  onResume: () => void
  onRestart: () => void
  onStop: () => void
  onMove: () => void
  onRename: () => void
  onArchive: () => void
  /** Open its window in ai-profiles. */
  onView: () => void
}

function RemoteSessionRow({
  host,
  session,
  email,
  home,
  busy,
  disabled,
  onResume,
  onRestart,
  onStop,
  onMove,
  onRename,
  onArchive,
  onView,
}: RemoteSessionRowProps) {
  const attach = attachCommand(host, session)
  const tmux = tmuxAttach(session)
  return (
    <SessionRowBase
      title={session.title ?? session.lastPrompt ?? session.id}
      badges={
        session.waiting ? (
          <span
            className="inline-flex shrink-0 items-center gap-1 text-meta text-muted-strong"
            title="Claude is asking something before it starts, such as whether to trust the folder."
          >
            <StatusDot tone="warning" />
            Waiting for you
          </span>
        ) : session.running ? (
          <>
            <span className="inline-flex shrink-0 items-center gap-1 text-meta text-muted-strong">
              <StatusDot tone={busy === 'stop' ? 'neutral' : 'success'} />
              {busy === 'stop' ? 'Stopping…' : tmux ? 'Open' : 'Open outside tmux'}
            </span>
            {session.updatePending && busy !== 'stop' ? (
              <span
                className="inline-flex shrink-0 items-center gap-1 text-meta text-amber"
                title={`It runs Claude Code ${session.claudeVersion ?? 'an older version'}, and a newer one is installed on the host. Restart it to update.`}
              >
                <RotateCw aria-hidden className="h-3 w-3" />
                Restart to update
              </span>
            ) : null}
          </>
        ) : null
      }
      folder={session.cwd}
      home={home}
      at={session.updatedAt}
      actions={
        session.waiting ? (
          <>
            <Button
              variant="ghost"
              size="sm"
              leadingIcon={<MessageSquareReply className="h-3.5 w-3.5" />}
              title="See what Claude is asking, and answer it"
              disabled={disabled}
              className={rowActionClasses}
              onClick={onResume}
            >
              {busy === 'resume' ? 'Opening…' : 'Answer'}
            </Button>
            <StopButton stopping={busy === 'stop'} disabled={disabled} onStop={onStop} />
          </>
        ) : session.running ? (
          <>
            {session.remoteControl && session.bridgeSessionId ? (
              <OpenInClaudeButton email={email} bridgeSessionId={session.bridgeSessionId} />
            ) : session.remoteControlConnecting ? (
              <span
                role="status"
                aria-label="Remote Control is connecting"
                title="Remote Control is connecting. Open in Claude shows once it has."
                className="inline-flex h-7 shrink-0 items-center justify-center rounded-md border border-blue/45 bg-blue/[0.12] px-2.5 text-blue"
              >
                <LoaderCircle aria-hidden className="h-3.5 w-3.5 animate-spin" />
              </span>
            ) : null}
            {attach && tmux ? (
              <Button
                variant="ghost"
                size="sm"
                aria-label="Open its window"
                title="Open its tmux window, here or in Terminal"
                onClick={onView}
              >
                <SquareTerminal aria-hidden className="h-3.5 w-3.5" />
              </Button>
            ) : null}
            <Button
              variant="ghost"
              size="sm"
              aria-label="Rename"
              title="Rename: here, in the Claude app and on the host"
              disabled={disabled}
              className={rowActionClasses}
              onClick={onRename}
            >
              <Pencil aria-hidden className="h-3.5 w-3.5" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              aria-label="Move to another profile"
              title="Move to another profile"
              disabled={disabled}
              className={rowActionClasses}
              onClick={onMove}
            >
              <ArrowRightLeft aria-hidden className="h-3.5 w-3.5" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              aria-label="Restart"
              title={
                session.updatePending
                  ? `Restart to update: stop it and start it again on the newer claude installed on the host (it runs ${session.claudeVersion ?? 'an older one'})`
                  : tmux
                    ? "Restart: stop it and start it again on the host's current claude"
                    : "Restart: stop it and start it again in tmux, on the host's current claude"
              }
              disabled={disabled}
              className={cn(rowActionClasses, session.updatePending && 'text-amber hover:text-amber')}
              onClick={onRestart}
            >
              <RotateCw aria-hidden className={cn('h-3.5 w-3.5', busy === 'restart' && 'animate-spin')} />
            </Button>
            <StopButton stopping={busy === 'stop'} disabled={disabled} onStop={onStop} />
          </>
        ) : (
          <>
            <Button
              variant="ghost"
              size="sm"
              aria-label="Rename"
              title="Rename: here, in the Claude app and on the host"
              disabled={disabled}
              className={rowActionClasses}
              onClick={onRename}
            >
              <Pencil aria-hidden className="h-3.5 w-3.5" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              aria-label="Move to another profile"
              title="Move to another profile"
              disabled={disabled}
              className={rowActionClasses}
              onClick={onMove}
            >
              <ArrowRightLeft aria-hidden className="h-3.5 w-3.5" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              aria-label="Archive"
              title="Archive"
              disabled={disabled}
              className={rowActionClasses}
              onClick={onArchive}
            >
              <Archive aria-hidden className="h-3.5 w-3.5" />
            </Button>
            {/* Last, under Stop in the rows above. */}
            <Button
              variant="ghost"
              size="sm"
              aria-label={busy === 'resume' ? 'Resuming' : 'Resume'}
              title={busy === 'resume' ? 'Resuming…' : 'Resume'}
              disabled={disabled}
              className={rowActionClasses}
              onClick={onResume}
            >
              {busy === 'resume' ? (
                <LoaderCircle aria-hidden className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Play aria-hidden className="h-3.5 w-3.5" />
              )}
            </Button>
          </>
        )
      }
    />
  )
}

/**
 * Renames a session everywhere it's called something: here, on the host,
 * and in the Claude app over Remote Control. A running one is renamed by
 * Claude, so it can come back with a suffix when another live session on
 * the host holds the name.
 */
function RenameRemoteSessionDialog({
  host,
  account,
  session,
  onClose,
}: {
  host: RemoteHost
  account: string
  session: RemoteSession
  onClose: () => void
}) {
  const [name, setName] = useState(session.named ? (session.title ?? '') : '')
  const rename = useRenameRemoteSession(host.id, account)
  const toast = useToast()
  const trimmed = name.trim()
  const ready = trimmed.length > 0 && trimmed.length <= 100 && !rename.isPending

  async function handleSave() {
    if (!ready) {
      return
    }
    try {
      const result = await rename.mutateAsync({ sessionId: session.id, name: trimmed })
      onClose()
      if (result.name !== trimmed) {
        toast.info(
          `Renamed to ${result.name}`,
          `Another session running on ${host.label} is called ${trimmed}, so Claude added a suffix.`,
        )
      } else {
        toast.success(
          `Renamed to ${result.name}`,
          result.live ? 'In the Claude app too.' : 'The Claude app shows it once the session is resumed.',
        )
      }
    } catch (caught) {
      toast.error('Could not rename it.', sessionErrorMessage(caught))
    }
  }

  return (
    <Dialog
      open
      title="Rename session"
      description={session.cwd ? shortenHomePath(session.cwd) : undefined}
      onClose={onClose}
      onSubmit={handleSave}
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={rename.isPending} onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            size="sm"
            trailingKbd={<Kbd variant="onOrange">⏎</Kbd>}
            disabled={!ready}
            onClick={handleSave}
          >
            {rename.isPending ? 'Renaming…' : 'Rename'}
          </Button>
        </>
      }
    >
      <div>
        <label htmlFor="remote-session-name" className="mb-1.5 block text-meta text-ink-soft">
          Name
        </label>
        <Input
          id="remote-session-name"
          autoFocus
          value={name}
          placeholder={session.title ?? session.lastPrompt ?? undefined}
          onChange={(event) => setName(event.target.value)}
          autoComplete="off"
          spellCheck={false}
        />
        <span className="mt-1.5 block text-meta text-muted-strong">
          {session.running
            ? 'Claude takes it now, and so does the Claude app. It has to be idle, at an empty prompt.'
            : 'Written to the session now. The Claude app shows it once the session is resumed.'}
        </span>
      </div>
    </Dialog>
  )
}

/**
 * Opens the session's Remote Control view in the Claude app on this Mac that
 * is signed in as the profile's account, starting it if it isn't running, or
 * on claude.ai when none is, or it can't be reached on its own. Shown only
 * while Remote Control is connected, in its own blue, so it also says that
 * the session is in the Claude app.
 */
function OpenInClaudeButton({ email, bridgeSessionId }: { email: string | null; bridgeSessionId: string }) {
  const toast = useToast()
  const [opening, setOpening] = useState(false)
  return (
    <Button
      variant="ghost"
      size="sm"
      aria-label="Open in Claude"
      title="Remote Control is connected. Open it in the Claude app signed in as this profile."
      className="border border-blue/45 bg-blue/[0.12] text-blue hover:bg-blue/20 hover:text-blue"
      disabled={opening}
      onClick={async () => {
        setOpening(true)
        try {
          const opened = await remoteOpenInClaude({ email, bridgeSessionId })
          if (opened.app) {
            toast.success(`Opened in ${opened.app}`)
          } else {
            toast.info(
              'Opened on claude.ai',
              opened.note ??
                (email ? `No Claude app here is signed in as ${email}.` : 'This profile has no account on record.'),
            )
          }
        } catch (caught) {
          toast.error('Could not open it.', sessionErrorMessage(caught))
        } finally {
          setOpening(false)
        }
      }}
    >
      <Laptop aria-hidden className="h-3.5 w-3.5" />
    </Button>
  )
}
