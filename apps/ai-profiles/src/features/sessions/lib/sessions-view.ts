import type { AppId } from '@/lib/app-registry'
import type { Session } from '@/lib/types'
import type { DescribedFailure } from './describe-failure'
import type { KindFilter, SessionsTab, SortDirection } from './session-filters'

import { appSpecs } from '@/lib/app-registry'

import { describeFailure } from './describe-failure'
import { countByTab, filterSessions, hasBothKinds, sortSessions } from './session-filters'

/**
 * Where the profile's listing stands.
 */
export type SessionsListing = {
  /**
   * Whether the latest listing failed.
   */
  isError: boolean
  /**
   * Why it failed, when it did.
   */
  error: unknown
  /**
   * The sessions listed; absent until the first listing lands.
   */
  sessions?: Array<Session>
}

/**
 * What the user narrowed the panel to.
 */
export type ViewFilters = {
  /**
   * The open tab.
   */
  tab: SessionsTab
  /**
   * The kind picked on the kind filter, set aside while the filter is hidden.
   */
  kindChoice: KindFilter
  /**
   * The search text.
   */
  query: string
  /**
   * The last-used order.
   */
  direction: SortDirection
}

/**
 * What the panel shows for a listing and the filters on it.
 */
type SessionsView = {
  /**
   * How many sessions the open tab holds before search and kind narrow it.
   */
  tabTotal: number
  /**
   * Whether the kind filter is offered: only while the open tab mixes both
   * kinds.
   */
  kindFilterShown: boolean
  /**
   * The kind the list is narrowed to — the one chosen, while the filter shows.
   */
  kind: KindFilter
  /**
   * The rows to show, filtered and ordered.
   */
  visible: Array<Session>
  /**
   * The sessions, on either tab, that need repair.
   */
  repairSessionIds: Array<string>
  /**
   * Why the listing failed, when there is nothing listed to show instead.
   */
  failure: DescribedFailure | null
  /**
   * Whether a refresh failed over rows that stay on screen.
   */
  refreshFailed: boolean
  /**
   * Whether the footer shows: once listed, while the open tab holds sessions.
   */
  footerShown: boolean
  /**
   * How many sessions each tab holds; absent until the first listing lands.
   */
  counts?: Record<SessionsTab, number>
}

/**
 * What an empty tab says.
 */
type EmptyTabCopy = {
  /**
   * The notice's title.
   */
  title: string
  /**
   * A line under the title.
   */
  hint?: string
}

/**
 * What the panel shows for `listing` narrowed by `filters`. When the kind
 * filter goes away, the kind chosen on it is set aside rather than reset, so
 * the list shows everything and the choice comes back with the filter.
 */
export function sessionsView(listing: SessionsListing, filters: ViewFilters): SessionsView {
  const listed = listing.sessions
  const sessions = listed ?? []
  const { tab, query, direction } = filters
  const inTab = filterSessions(sessions, { tab, kind: 'all', query: '' })
  const kindFilterShown = hasBothKinds(inTab)
  const kind = kindFilterShown ? filters.kindChoice : 'all'
  return {
    tabTotal: inTab.length,
    kindFilterShown,
    kind,
    visible: sortSessions(filterSessions(inTab, { tab, kind, query }), direction),
    repairSessionIds: sessions.filter((session) => session.needsRepair).map((session) => session.id),
    // A failed refetch keeps the rows it already has, under a quiet note; only
    // a listing that never landed shows the failure.
    failure: listed === undefined && listing.isError ? describeFailure(listing.error) : null,
    refreshFailed: listed !== undefined && listing.isError,
    footerShown: listed !== undefined && inTab.length > 0,
    counts: listed === undefined ? undefined : countByTab(listed),
  }
}

/**
 * What `tab` says while it holds no sessions of `app`: on the Active tab, a
 * hint that the CLI's sessions land there.
 */
export function emptyTabCopy(tab: SessionsTab, app: AppId): EmptyTabCopy {
  if (tab === 'archived') {
    return { title: 'No archived sessions' }
  }
  return {
    title: 'No sessions yet',
    hint: `${appSpecs[app].cliDisplayName} sessions this profile starts show up here.`,
  }
}
