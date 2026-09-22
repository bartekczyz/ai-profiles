import type { SessionSummary } from '@/lib/types'

import { useState } from 'react'

import { formatDistanceToNow } from 'date-fns'
import { ArrowRightLeft, Monitor, Terminal } from 'lucide-react'

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
 * profile. Lists the CLI transcripts, which is where every session lives: a
 * desktop Code tab session is one too, and wears a Desktop pill instead of a
 * CLI one.
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

/**
 * Where a session lives: the desktop app if its Code tab lists the session
 * (or holds it open), the CLI otherwise.
 */
function surfaceOf(session: SessionSummary): 'desktop' | 'cli' {
  if (session.running) {
    return session.openInDesktop ? 'desktop' : 'cli'
  }
  return session.inDesktop ? 'desktop' : 'cli'
}

function SurfacePill({ surface }: { surface: 'desktop' | 'cli' }) {
  const Icon = surface === 'desktop' ? Monitor : Terminal
  return (
    <span className="inline-flex shrink-0 items-center gap-1 rounded-[5px] border border-border-soft px-1.5 py-px font-mono text-[10px] font-medium uppercase leading-[1.5] tracking-[0.08em] text-muted-strong">
      <Icon aria-hidden strokeWidth={1.75} className="h-2.5 w-2.5" />
      {surface === 'desktop' ? 'Desktop' : 'CLI'}
    </span>
  )
}

function SessionRow({ session, onMove }: { session: SessionSummary; onMove: () => void }) {
  const title = session.title ?? session.lastPrompt ?? session.id
  return (
    <li className={rowClasses}>
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5">
          <span className="truncate text-body text-ink" title={title}>
            {title}
          </span>
          <SurfacePill surface={surfaceOf(session)} />
        </div>
        <div className="flex min-w-0 font-mono text-mono text-muted-strong">
          <span className="truncate" title={session.cwd ?? undefined}>
            {session.cwd ? shortenHomePath(session.cwd) : 'unknown folder'}
          </span>
          <span className="shrink-0 whitespace-nowrap">
            <span className="mx-1.5 text-border">·</span>
            {formatDistanceToNow(new Date(session.updatedAt), { addSuffix: true })}
          </span>
        </div>
      </div>
      {session.running ? (
        <div
          className="shrink-0 text-right"
          title={session.openInDesktop ? 'The desktop app holds it open until it quits.' : 'A terminal has it open.'}
        >
          <div className="flex items-center justify-end gap-1.5 text-meta text-ink-soft">
            <StatusDot tone="success" />
            Open
          </div>
          <div className="text-meta text-muted">Quit to move</div>
        </div>
      ) : session.unmovableReason ? (
        <span className="shrink-0 cursor-default text-meta text-muted" title={session.unmovableReason}>
          Can't move
        </span>
      ) : (
        <Button variant="ghost" size="sm" leadingIcon={<ArrowRightLeft />} onClick={onMove}>
          Move
        </Button>
      )}
    </li>
  )
}
