import type { SessionSummary } from '@/lib/types'

import { useState } from 'react'

import { formatDistanceToNow } from 'date-fns'
import { ArrowRightLeft, Monitor } from 'lucide-react'

import { Button, Skeleton, StatusDot } from '@/design'
import { extractErrorMessage } from '@/lib/extract-error-message'

import { useProfileSessions } from '../api/use-profile-sessions'
import { shortenHomePath } from './shorten-home-path'
import { TransferSessionDialog } from './transfer-session-dialog'

/** Rows shown before "Show all". */
const collapsedCount = 5

const panelClasses = 'rounded-[10px] border border-border-soft bg-white/30 dark:bg-white/[0.02]'

const rowClasses =
  'flex min-h-[46px] items-center justify-between gap-3 border-t border-border-soft px-[13px] py-[8px] first:border-t-0'

type Props = {
  /** Profile id, or `default:claude` for the stock install. */
  profileId: string
}

/**
 * The Claude sessions this profile keeps, with a way to move one to another
 * profile. Lists the CLI transcripts, which is where every session lives:
 * a desktop Code tab session is one too, and says so with a monitor icon.
 */
export function ProfileDetailSessions({ profileId }: Props) {
  const { data, error, isLoading } = useProfileSessions(profileId)
  const [expanded, setExpanded] = useState(false)
  const [moving, setMoving] = useState<SessionSummary | null>(null)

  const sessions = data ?? []
  const visible = expanded ? sessions : sessions.slice(0, collapsedCount)

  return (
    <section aria-label="Sessions" className="mb-6">
      <div className="mb-2 flex items-baseline justify-between px-0.5">
        <h2 className="text-meta font-medium text-ink-soft">Sessions</h2>
        {data ? <span className="text-meta text-muted">{sessions.length}</span> : null}
      </div>
      <div className={panelClasses}>
        {isLoading ? (
          <div className={rowClasses}>
            <Skeleton shape="text" className="w-2/3" />
          </div>
        ) : error ? (
          <p role="alert" className="px-[13px] py-[10px] text-meta text-red">
            {extractErrorMessage(error, 'Could not read the sessions.')}
          </p>
        ) : sessions.length === 0 ? (
          <p className="px-[13px] py-[10px] text-meta text-muted">No sessions yet.</p>
        ) : (
          <ul>
            {visible.map((session) => (
              <SessionRow key={session.id} session={session} onMove={() => setMoving(session)} />
            ))}
          </ul>
        )}
      </div>
      {sessions.length > collapsedCount ? (
        <button
          type="button"
          className="mt-1.5 cursor-pointer px-0.5 text-meta text-muted-strong hover:text-ink"
          onClick={() => setExpanded((value) => !value)}
        >
          {expanded ? 'Show fewer' : `Show all ${sessions.length}`}
        </button>
      ) : null}

      {moving ? (
        <TransferSessionDialog open sourceId={profileId} session={moving} onClose={() => setMoving(null)} />
      ) : null}
    </section>
  )
}

function SessionRow({ session, onMove }: { session: SessionSummary; onMove: () => void }) {
  const title = session.title ?? session.lastPrompt ?? session.id
  const blocked = session.running
    ? session.openInDesktop
      ? 'The desktop app has it open. Quit the app to move it.'
      : 'Open in a terminal. Close it to move it.'
    : session.unmovableReason
  return (
    <li className={rowClasses}>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          {session.running ? (
            <span role="img" aria-label="Open right now" className="inline-flex">
              <StatusDot tone="success" />
            </span>
          ) : null}
          <span className="truncate text-body text-ink" title={title}>
            {title}
          </span>
          {session.inDesktop ? (
            <Monitor aria-label="In the desktop app" className="h-3 w-3 shrink-0 text-muted" />
          ) : null}
        </div>
        <div className="truncate font-mono text-mono text-muted-strong">
          {session.cwd ? shortenHomePath(session.cwd) : 'unknown folder'}
          <span className="mx-1.5 text-border">·</span>
          {formatDistanceToNow(new Date(session.updatedAt), { addSuffix: true })}
        </div>
      </div>
      {blocked ? (
        <span className="shrink-0 cursor-default text-meta text-muted" title={blocked}>
          {session.running ? (session.openInDesktop ? 'Open in the app' : 'Open now') : "Can't move"}
        </span>
      ) : (
        <Button variant="ghost" size="sm" leadingIcon={<ArrowRightLeft />} onClick={onMove}>
          Move
        </Button>
      )}
    </li>
  )
}
