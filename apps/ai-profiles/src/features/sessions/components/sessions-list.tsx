import type { ReactNode } from 'react'
import type { AppId, Session, SessionAction } from '@/lib/types'
import type { DescribedFailure } from '../lib/describe-failure'
import type { MoveTarget } from './move-session-dialog'
import type { SessionRowAction } from './session-row-actions'

import { cn } from '@/design'
import { TooltipProvider } from '@/design/ui/tooltip'

import { moveAvailability, rowActions } from '../lib/session-actions'
import { SessionRow } from './session-row'
import { SessionsListSkeleton, sessionsListClasses } from './sessions-panel-skeleton'

type Props = {
  /**
   * Whether the first listing is still on its way — later refetches keep the
   * rows on screen instead.
   */
  loading: boolean
  /**
   * Whether a Retry is under way, which disables the button.
   */
  retrying: boolean
  /**
   * Why the listing failed, when there is nothing listed to show instead. A
   * missing tool shows as a notice, without a Retry; anything else as an
   * alert with one.
   */
  failure: DescribedFailure | null
  /**
   * How many sessions the open tab holds before search and kind narrow it.
   */
  tabTotal: number
  /**
   * The rows to show, filtered and ordered.
   */
  sessions: Array<Session>
  /**
   * The app the sessions are of.
   */
  app: AppId
  /**
   * The other profiles of the app a session can be moved to.
   */
  moveTargets: Array<MoveTarget>
  /**
   * What an empty tab says.
   */
  emptyTitle: string
  /**
   * A line under an empty tab's title.
   */
  emptyHint?: string
  /**
   * Refetches after a failed listing.
   */
  onRetry: () => void
  /**
   * Empties the search, from the no-match state.
   */
  onClearSearch: () => void
  /**
   * Asks to do `action` to `session`, from its row.
   */
  onAction: (session: Session, action: SessionAction) => void
  /**
   * Asks to move `session` to `target`, from its row.
   */
  onMove: (session: Session, target: MoveTarget) => void
}

/**
 * What `session`'s row offers, given where it can move to.
 */
type RowActionsInput = {
  /**
   * The session the row shows.
   */
  session: Session
  /**
   * The app the session is of.
   */
  app: AppId
  /**
   * The other profiles of the app a session can be moved to.
   */
  moveTargets: Array<MoveTarget>
  /**
   * Asks to do `action` to the session.
   */
  onAction: (session: Session, action: SessionAction) => void
  /**
   * Asks to move the session to `target`.
   */
  onMove: (session: Session, target: MoveTarget) => void
}

type NoticeProps = {
  /**
   * The notice's content.
   */
  children: ReactNode
}

type ListErrorProps = {
  /**
   * Whether a Retry is under way.
   */
  retrying: boolean
  /**
   * Why the listing failed.
   */
  message: string
  /**
   * Refetches.
   */
  onRetry: () => void
}

/**
 * How each action is named on a row.
 */
const actionLabels: Record<SessionAction, string> = {
  archive: 'Archive',
  restore: 'Restore',
}

/**
 * The list's secondary buttons, Retry and Clear search: a quiet bordered
 * pill.
 */
const quietButtonClasses =
  'inline-flex h-7 shrink-0 cursor-pointer items-center rounded-[7px] border border-border bg-white/60 px-2.5 text-[12px] text-ink-soft outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:border-border-strong hover:bg-white focus-visible:ring-2 focus-visible:ring-orange/40 disabled:cursor-default disabled:opacity-60 dark:bg-white/[0.05] dark:hover:bg-white/[0.09]'

/**
 * The open tab's body: a skeleton while the first listing loads, why the
 * profile can't list sessions, the failure with a Retry, an empty or no-match
 * notice, or the rows, which share one tooltip provider for why their
 * actions are held back.
 */
export function SessionsList({
  loading,
  retrying,
  failure,
  tabTotal,
  sessions,
  app,
  moveTargets,
  emptyTitle,
  emptyHint,
  onRetry,
  onClearSearch,
  onAction,
  onMove,
}: Props) {
  if (loading) {
    return <SessionsListSkeleton />
  }
  if (failure?.missingTool) {
    return (
      <Notice>
        <p className="text-[12.5px] text-ink">{failure.message}</p>
      </Notice>
    )
  }
  if (failure !== null) {
    return <ListError retrying={retrying} message={failure.message} onRetry={onRetry} />
  }
  if (tabTotal === 0) {
    return (
      <Notice>
        <p className="text-[12.5px] text-ink">{emptyTitle}</p>
        {emptyHint === undefined ? null : <p className="mt-0.5 text-[11px] text-muted-strong">{emptyHint}</p>}
      </Notice>
    )
  }
  if (sessions.length === 0) {
    return (
      <Notice>
        <p className="text-[12.5px] text-ink">No sessions match</p>
        <button type="button" className={cn(quietButtonClasses, 'mt-2')} onClick={onClearSearch}>
          Clear search
        </button>
      </Notice>
    )
  }
  return (
    <TooltipProvider>
      <ul aria-label="Sessions" className={sessionsListClasses}>
        {sessions.map((session) => (
          <SessionRow
            key={session.id}
            session={session}
            app={app}
            actions={sessionRowActions({ session, app, moveTargets, onAction, onMove })}
          />
        ))}
      </ul>
    </TooltipProvider>
  )
}

/**
 * The actions `session`'s row offers: Move, when the session can go to
 * another profile, then Archive or Restore.
 */
function sessionRowActions({ session, app, moveTargets, onAction, onMove }: RowActionsInput): Array<SessionRowAction> {
  const actions: Array<SessionRowAction> = rowActions(session, app).map((item) => ({
    id: item.action,
    label: actionLabels[item.action],
    disabledReason: item.disabledReason,
    onSelect: () => onAction(session, item.action),
  }))
  const move = moveAvailability(session, moveTargets.length)
  if (move === null) {
    return actions
  }
  const moveAction: SessionRowAction = {
    id: 'move',
    label: 'Move',
    disabledReason: move.disabledReason,
    targets: moveTargets,
    onSelect: (targetId) => {
      const target = moveTargets.find((candidate) => candidate.id === targetId)
      if (target !== undefined) {
        onMove(session, target)
      }
    },
  }
  return [moveAction, ...actions]
}

/**
 * A failed listing: the reason, inline, with a Retry, where the rows would be.
 */
function ListError({ retrying, message, onRetry }: ListErrorProps) {
  return (
    <div
      role="alert"
      className="flex flex-1 items-start justify-between gap-3 border-t border-border-soft px-[13px] py-[9px]"
    >
      <p className="min-w-0 py-1.5 text-[12px] text-red">{message}</p>
      <button type="button" disabled={retrying} className={quietButtonClasses} onClick={onRetry}>
        Retry
      </button>
    </div>
  )
}

/**
 * A centred message in place of the rows.
 */
function Notice({ children }: NoticeProps) {
  return (
    <div className="flex-1 border-t border-border-soft p-3">
      <div className="flex flex-col items-center rounded-[10px] border border-dashed border-border px-[13px] py-6 text-center">
        {children}
      </div>
    </div>
  )
}
