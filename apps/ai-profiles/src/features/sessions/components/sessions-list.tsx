import type { ReactNode } from 'react'
import type { Session, SessionAction } from '@/lib/types'

import { cn } from '@/design'

import { rowActions } from '../lib/session-actions'
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
   * Why the listing failed, when there is nothing listed to show instead.
   */
  errorMessage: string | null
  /**
   * How many sessions the open tab holds before search and kind narrow it.
   */
  tabTotal: number
  /**
   * The rows to show, filtered and ordered.
   */
  sessions: Array<Session>
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

const quietButtonClasses =
  'inline-flex h-7 shrink-0 cursor-pointer items-center rounded-[7px] border border-border bg-white/60 px-2.5 text-[12px] text-ink-soft outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:border-border-strong hover:bg-white focus-visible:ring-2 focus-visible:ring-orange/40 disabled:cursor-default disabled:opacity-60 dark:bg-white/[0.05] dark:hover:bg-white/[0.09]'

/**
 * The open tab's body: a skeleton while the first listing loads, the failure
 * with a Retry, an empty or no-match notice, or the rows.
 */
export function SessionsList({
  loading,
  retrying,
  errorMessage,
  tabTotal,
  sessions,
  emptyTitle,
  emptyHint,
  onRetry,
  onClearSearch,
  onAction,
}: Props) {
  if (loading) {
    return <SessionsListSkeleton />
  }
  if (errorMessage !== null) {
    return <ListError retrying={retrying} message={errorMessage} onRetry={onRetry} />
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
    <ul aria-label="Sessions" className={sessionsListClasses}>
      {sessions.map((session) => (
        <SessionRow
          key={session.id}
          session={session}
          actions={rowActions(session).map((item) => ({
            id: item.action,
            label: actionLabels[item.action],
            disabledReason: item.disabledReason,
            onSelect: () => onAction(session, item.action),
          }))}
        />
      ))}
    </ul>
  )
}

/**
 * A failed listing: the reason, inline, with a Retry.
 */
function ListError({ retrying, message, onRetry }: ListErrorProps) {
  return (
    <div
      role="alert"
      className="flex items-center justify-between gap-3 rounded-[10px] border border-border-soft px-[13px] py-[9px]"
    >
      <p className="min-w-0 text-[12px] text-red">{message}</p>
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
    <div className="flex flex-col items-center rounded-[10px] border border-dashed border-border px-[13px] py-6 text-center">
      {children}
    </div>
  )
}
