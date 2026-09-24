import type { Session, SessionKind } from '@/lib/types'

/**
 * The panel's two tabs: sessions in use, and sessions put away.
 */
export type SessionsTab = 'active' | 'archived'

/**
 * Which kinds of session the list shows.
 */
export type KindFilter = 'all' | SessionKind

/**
 * Last-used order: `desc` puts the most recently used first.
 */
export type SortDirection = 'desc' | 'asc'

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
 * A copy of `sessions` ordered by when each was last used.
 */
export function sortSessions(sessions: Array<Session>, direction: SortDirection): Array<Session> {
  const sign = direction === 'desc' ? -1 : 1
  return [...sessions].sort((left, right) => sign * (Date.parse(left.lastUsedAt) - Date.parse(right.lastUsedAt)))
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
