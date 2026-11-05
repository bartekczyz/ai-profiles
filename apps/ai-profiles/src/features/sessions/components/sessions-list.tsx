import type { ReactNode } from 'react'
import type { AppId, Session, SessionAction, SessionCost } from '@/lib/types'
import type { DescribedFailure } from '../lib/describe-failure'
import type { MoveTarget } from './move-session-dialog'
import type { SessionRowAction } from './session-row-actions'

import { ChatsCircle } from '@phosphor-icons/react'

import { cn } from '@/design'
import { TooltipProvider } from '@/design/ui/tooltip'

import { moveAvailability, rowActions } from '../lib/session-actions'
import { SessionRow, sessionRowGridClasses } from './session-row'
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
   * What the sessions would cost at API prices, as far as it is read.
   */
  costs: RowCosts
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

/**
 * What the sessions would cost at API prices, as far as it is read.
 */
type RowCosts = {
  /**
   * Whether the costs are still being read.
   */
  pending: boolean
  /**
   * Each session's, by its id; absent until read, or when they couldn't be.
   */
  byId?: Record<string, SessionCost>
}

type SessionRowsProps = {
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
   * What the sessions would cost at API prices, as far as it is read.
   */
  costs: RowCosts
  /**
   * Asks to do `action` to `session`, from its row.
   */
  onAction: (session: Session, action: SessionAction) => void
  /**
   * Asks to move `session` to `target`, from its row.
   */
  onMove: (session: Session, target: MoveTarget) => void
}

type EmptyNoticeProps = {
  /**
   * What the empty tab says.
   */
  title: string
  /**
   * A line under the title.
   */
  hint?: string
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
 * The rows' box, laid out in the rows' three columns: the session, its cost
 * and its actions. Rows start at the top rather than share out its height,
 * and each is as tall as its content: once the rows overflow the box, a grid
 * row would otherwise shrink to the row's minimum height and cut it off.
 */
const rowsGridClasses = 'grid auto-rows-max grid-cols-[minmax(0,1fr)_auto_auto] content-start gap-x-3'

/**
 * The list's secondary buttons, Retry and Clear search: a quiet bordered
 * pill.
 */
const quietButtonClasses =
  'inline-flex h-7 shrink-0 items-center rounded-[7px] border border-border bg-white/60 px-2.5 text-[12px] text-ink-soft outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:border-border-strong hover:bg-white focus-visible:ring-2 focus-visible:ring-orange/40 disabled:cursor-default disabled:opacity-60 dark:bg-white/[0.05] dark:hover:bg-white/[0.09]'

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
  costs,
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
        <p className="text-[13px] font-medium text-ink">{failure.message}</p>
      </Notice>
    )
  }
  if (failure !== null) {
    return <ListError retrying={retrying} message={failure.message} onRetry={onRetry} />
  }
  if (tabTotal === 0) {
    return <EmptyNotice title={emptyTitle} hint={emptyHint} />
  }
  if (sessions.length === 0) {
    return (
      <Notice>
        <p className="text-[13px] font-medium text-ink">No sessions match</p>
        <button type="button" className={cn(quietButtonClasses, 'mt-2')} onClick={onClearSearch}>
          Clear search
        </button>
      </Notice>
    )
  }
  return (
    <SessionRows
      sessions={sessions}
      app={app}
      moveTargets={moveTargets}
      costs={costs}
      onAction={onAction}
      onMove={onMove}
    />
  )
}

/**
 * The rows, under heads naming the session and cost columns, which share one
 * tooltip provider for their costs' breakdowns and why their actions are held
 * back. The heads show while the costs are read and once some row has one.
 */
function SessionRows({ sessions, app, moveTargets, costs, onAction, onMove }: SessionRowsProps) {
  const costShown = costs.pending || sessions.some((session) => costs.byId?.[session.id] !== undefined)
  return (
    <TooltipProvider>
      <ul aria-label="Sessions" className={cn(sessionsListClasses, rowsGridClasses)}>
        {costShown ? <ColumnHeads /> : null}
        {sessions.map((session) => (
          <SessionRow
            key={session.id}
            costPending={costs.pending}
            session={session}
            app={app}
            cost={costs.byId?.[session.id]}
            actions={sessionRowActions({ session, app, moveTargets, onAction, onMove })}
          />
        ))}
      </ul>
    </TooltipProvider>
  )
}

/**
 * The heads of the session and cost columns, as the list's first row. Hidden
 * from screen readers: each row's cost says what it is.
 */
function ColumnHeads() {
  return (
    <li aria-hidden className={cn(sessionRowGridClasses, 'py-[5px] font-mono text-[11px] text-muted-strong')}>
      <span>Session</span>
      <span className="text-right">API-equivalent</span>
      <span />
    </li>
  )
}

/**
 * An empty tab: what it says, and a line under that when there is one.
 */
function EmptyNotice({ title, hint }: EmptyNoticeProps) {
  return (
    <Notice>
      <p className="text-[13px] font-medium text-ink">{title}</p>
      {hint === undefined ? null : <p className="mt-1 text-[12px] text-muted">{hint}</p>}
    </Notice>
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
 * A message in place of the rows, centred in the space they would fill: a
 * quiet glyph over the text, in the manner of an empty Mail or Notes list.
 */
function Notice({ children }: NoticeProps) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 border-t border-border-soft px-6 py-10 text-center">
      <span
        aria-hidden
        className="grid size-10 place-items-center rounded-xl border border-border bg-cream-2 text-muted-strong dark:bg-white/[0.04]"
      >
        <ChatsCircle className="size-5" />
      </span>
      <div className="flex max-w-[340px] flex-col items-center">{children}</div>
    </div>
  )
}
