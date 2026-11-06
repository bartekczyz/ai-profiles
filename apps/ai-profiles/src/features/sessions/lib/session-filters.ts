import type { Session, SessionCost, SessionKind } from '@/lib/types'

import { shownUsd } from './session-cost'

/**
 * The panel's two tabs: sessions in use, and sessions put away.
 */
export type SessionsTab = 'active' | 'archived'

/**
 * Which kinds of session the list shows.
 */
export type KindFilter = 'all' | SessionKind

/**
 * Which way the list runs: `desc` puts the most recently used, or the
 * priciest, first.
 */
export type SortDirection = 'desc' | 'asc'

/**
 * What the list is ordered by: when each session was last used, or what it
 * would cost at API prices.
 */
export type SortKey = 'lastUsed' | 'cost'

/**
 * How the list is ordered.
 */
export type SessionSort = {
  /**
   * What it is ordered by.
   */
  key: SortKey
  /**
   * Which way it runs.
   */
  direction: SortDirection
}

/**
 * What narrows the list down.
 */
type FilterOptions = {
  /**
   * The open tab.
   */
  tab: SessionsTab
  /**
   * The kind to keep, or `all`.
   */
  kind: KindFilter
  /**
   * Free text matched against the title, folder and last prompt.
   */
  query: string
}

/**
 * The sessions on `tab`, of `kind`, whose title, folder or last prompt
 * contains `query` — case-insensitively, ignoring surrounding whitespace. A
 * blank query matches everything.
 */
export function filterSessions(sessions: Array<Session>, { tab, kind, query }: FilterOptions): Array<Session> {
  const needle = query.trim().toLowerCase()
  return sessions.filter((session) => {
    if (session.archived !== (tab === 'archived')) {
      return false
    }
    if (kind !== 'all' && session.kind !== kind) {
      return false
    }
    return needle === '' || searchableText(session).some((text) => text.toLowerCase().includes(needle))
  })
}

/**
 * A copy of `sessions` in `sort`'s order. By cost, read from `costs`, the
 * sessions without a figure go last whichever way it runs, and sessions of
 * the same cost go most recently used first.
 */
export function sortSessions(
  sessions: Array<Session>,
  sort: SessionSort,
  costs: Record<string, SessionCost> = {},
): Array<Session> {
  const sign = sort.direction === 'desc' ? -1 : 1
  if (sort.key === 'lastUsed') {
    return [...sessions].sort((left, right) => sign * recency(left, right))
  }
  return [...sessions].sort((left, right) => {
    const leftUsd = shownUsd(costs[left.id])
    const rightUsd = shownUsd(costs[right.id])
    if (leftUsd === null || rightUsd === null) {
      return Number(leftUsd === null) - Number(rightUsd === null) || -recency(left, right)
    }
    return sign * (leftUsd - rightUsd) || -recency(left, right)
  })
}

/**
 * How much later `left` was last used than `right`, in milliseconds.
 */
function recency(left: Session, right: Session): number {
  return Date.parse(left.lastUsedAt) - Date.parse(right.lastUsedAt)
}

/**
 * Whether `sessions` holds both desktop and CLI sessions — the only case in
 * which filtering by kind narrows anything.
 */
export function hasBothKinds(sessions: Array<Session>): boolean {
  return sessions.some((session) => session.kind === 'desktop') && sessions.some((session) => session.kind === 'cli')
}

/**
 * How many sessions each tab holds.
 */
export function countByTab(sessions: Array<Session>): Record<SessionsTab, number> {
  const archived = sessions.filter((session) => session.archived).length
  return { active: sessions.length - archived, archived }
}

/**
 * The fields a search looks in, skipping the empty ones.
 */
function searchableText(session: Session): Array<string> {
  return [session.title, session.cwd, session.lastPrompt].filter((text): text is string => text !== null)
}
