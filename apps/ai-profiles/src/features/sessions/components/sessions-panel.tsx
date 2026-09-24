import type { AppId } from '@/lib/app-registry'
import type { Session, SessionAction } from '@/lib/types'
import type { KindFilter, SessionsTab, SortDirection } from '../lib/session-filters'
import type { MoveTarget } from './move-session-dialog'

import { Suspense, useState } from 'react'

import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/design/ui/tabs'
import { appFromEntry, entryId, useSidebarEntries } from '@/features/profiles/api/use-sidebar-entries'
import { appSpecs } from '@/lib/app-registry'

import { useSessions } from '../api/use-sessions'
import { describeFailure } from '../lib/describe-failure'
import { sessionCount } from '../lib/session-count'
import { countByTab, filterSessions, hasBothKinds, sortSessions } from '../lib/session-filters'
import { ConfirmSessionActionDialog } from './confirm-session-action-dialog'
import { MoveSessionDialog } from './move-session-dialog'
import { RefreshFailedNote } from './refresh-failed-note'
import { RepairBanner } from './repair-banner'
import { SessionsControls } from './sessions-controls'
import { SessionsList } from './sessions-list'
import { sessionsHeaderClasses, sessionsPanelClasses } from './sessions-panel-skeleton'

type Props = {
  /**
   * The profile whose sessions to list — a managed profile's id, or
   * `default:<app>` for a stock install.
   */
  profileId: string
  /**
   * The app the profile runs.
   */
  app: AppId
}

/**
 * An action the user asked for from a row, waiting on their confirmation.
 */
type PendingAction = {
  /**
   * The session to act on.
   */
  session: Session
  /**
   * What to do to it.
   */
  action: SessionAction
}

/**
 * A move the user asked for from a row, waiting on their confirmation.
 */
type PendingMove = {
  /**
   * The session to move.
   */
  session: Session
  /**
   * Where to move it.
   */
  destination: MoveTarget
}

type TabLabelProps = {
  /**
   * The tab's name.
   */
  label: string
  /**
   * How many sessions the tab holds; absent until the first listing lands.
   */
  count?: number
}

type ListFooterProps = {
  /**
   * How many rows show.
   */
  shown: number
  /**
   * How many sessions the open tab holds before search and kind narrow it.
   */
  total: number
  /**
   * The last-used order.
   */
  direction: SortDirection
}

/**
 * A tab's body: a column that can shrink, so only its rows scroll.
 */
const tabContentClasses = 'flex min-h-0 flex-col'

/**
 * The Active / Archived tabs: a small segmented control in the card's
 * eyebrow row.
 */
const tabListClasses = 'h-auto gap-[2px] rounded-[7px] border border-border bg-cream-2 p-[2px]'

/**
 * An Active or Archived tab: a mono segment, raised while selected.
 */
const tabTriggerClasses =
  'h-[18px] flex-none rounded-[5px] px-[7px] font-mono text-[10.5px] text-muted hover:text-ink data-active:bg-cream data-active:text-ink data-active:shadow-[0_1px_2px_rgba(0,0,0,0.08),inset_0_0_0_1px_var(--color-border)] dark:data-active:border-transparent dark:data-active:bg-cream'

/**
 * What the footer says for each order.
 */
const directionLabels: Record<SortDirection, string> = {
  desc: 'newest first',
  asc: 'oldest first',
}

/**
 * The sessions a profile owns, beside (or, in a narrow pane, below) its
 * details, in a card that speaks the Usage card's language.
 *
 * A SESSIONS eyebrow heads the card with the Active and Archived tabs across
 * from it; under them, one toolbar — search, the Desktop/CLI filter and the
 * last-used sort — applies to whichever tab is open, and a footer says how
 * many rows show and in what order. The panel fills the height the pane gives it — the column
 * beside the details, or the room under them — and only the rows scroll.
 *
 * An active session's row offers Move, a menu of the app's other profiles
 * (Default included); picking one asks to confirm the move. While some
 * sessions need repair, a banner heads the Active tab offering to repair them.
 * A refresh that fails leaves the last list on screen under a quiet note
 * with a Retry.
 *
 * The kind filter only exists while the open tab mixes both kinds. When it
 * goes away, the choice made on it is set aside rather than reset, so the
 * list shows everything and the choice comes back with the filter.
 */
export function SessionsPanel({ profileId, app }: Props) {
  const sessionsQuery = useSessions(profileId)
  const [tab, setTab] = useState<SessionsTab>('active')
  const [kindChoice, setKindChoice] = useState<KindFilter>('all')
  const [query, setQuery] = useState('')
  const [direction, setDirection] = useState<SortDirection>('desc')
  const [pendingAction, setPendingAction] = useState<PendingAction | null>(null)
  const [pendingMove, setPendingMove] = useState<PendingMove | null>(null)
  const profiles: Array<MoveTarget> = useSidebarEntries()
    .filter((entry) => appFromEntry(entry) === app)
    .map((entry) => ({
      id: entryId(entry),
      label: entry.kind === 'managed' ? entry.profile.name : entry.entry.name,
    }))
  const moveTargets = profiles.filter((profile) => profile.id !== profileId)
  const profileLabel = profiles.find((profile) => profile.id === profileId)?.label ?? appSpecs[app].displayName

  const listed = sessionsQuery.data?.sessions
  const sessions = listed ?? []
  const counts = countByTab(sessions)
  const inTab = filterSessions(sessions, { tab, kind: 'all', query: '' })
  const kindFilterShown = hasBothKinds(inTab)
  const kind = kindFilterShown ? kindChoice : 'all'
  const visible = sortSessions(filterSessions(inTab, { tab, kind, query }), direction)
  // A failed refetch keeps the rows it already has, under a quiet note; only
  // a listing that never landed shows the failure.
  const failure = listed === undefined && sessionsQuery.isError ? describeFailure(sessionsQuery.error) : null
  const refreshFailed = listed !== undefined && sessionsQuery.isError
  const retry = () => {
    void sessionsQuery.refetch()
  }

  return (
    <Tabs
      value={tab}
      className={sessionsPanelClasses}
      onValueChange={(value) => setTab(value === 'archived' ? 'archived' : 'active')}
    >
      <header className={sessionsHeaderClasses}>
        <h2 className="font-mono text-eyebrow font-medium uppercase tracking-[0.1em] text-muted-strong">Sessions</h2>
        <TabsList aria-label="Sessions" className={tabListClasses}>
          <TabsTrigger value="active" className={tabTriggerClasses}>
            <TabLabel label="Active" count={listed === undefined ? undefined : counts.active} />
          </TabsTrigger>
          <TabsTrigger value="archived" className={tabTriggerClasses}>
            <TabLabel label="Archived" count={listed === undefined ? undefined : counts.archived} />
          </TabsTrigger>
        </TabsList>
      </header>
      <SessionsControls
        kindFilterShown={kindFilterShown}
        query={query}
        kind={kind}
        direction={direction}
        onQueryChange={setQuery}
        onKindChange={setKindChoice}
        onDirectionChange={setDirection}
      />
      {/* One content slot, for whichever tab is open: the rows below the
          controls are the same list either way, filtered by the tab. Keyed
          by the tab, so switching starts it afresh as separate tabs would. */}
      <TabsContent key={tab} value={tab} className={tabContentClasses}>
        {/* Its own boundary: the banner waits on the app state for what was
            dismissed, and the rows shouldn't wait with it. */}
        {tab === 'active' ? (
          <Suspense fallback={null}>
            <RepairBanner
              profileId={profileId}
              profileLabel={profileLabel}
              repairSessionIds={sessions.filter((session) => session.needsRepair).map((session) => session.id)}
            />
          </Suspense>
        ) : null}
        {refreshFailed ? <RefreshFailedNote retrying={sessionsQuery.isFetching} onRetry={retry} /> : null}
        <SessionsList
          loading={sessionsQuery.isPending}
          retrying={sessionsQuery.isFetching}
          failure={failure}
          tabTotal={inTab.length}
          sessions={visible}
          app={app}
          moveTargets={moveTargets}
          emptyTitle={tab === 'active' ? 'No sessions yet' : 'No archived sessions'}
          emptyHint={
            tab === 'active' ? `${appSpecs[app].cliDisplayName} sessions this profile starts show up here.` : undefined
          }
          onRetry={retry}
          onClearSearch={() => setQuery('')}
          onAction={(session, action) => setPendingAction({ session, action })}
          onMove={(session, destination) => setPendingMove({ session, destination })}
        />
      </TabsContent>
      {listed === undefined || inTab.length === 0 ? null : (
        <ListFooter shown={visible.length} total={inTab.length} direction={direction} />
      )}
      {pendingAction === null ? null : (
        <ConfirmSessionActionDialog
          profileId={profileId}
          session={pendingAction.session}
          action={pendingAction.action}
          onClose={() => setPendingAction(null)}
        />
      )}
      {pendingMove === null ? null : (
        <MoveSessionDialog
          profileId={profileId}
          session={pendingMove.session}
          destination={pendingMove.destination}
          onClose={() => setPendingMove(null)}
        />
      )}
    </Tabs>
  )
}

/**
 * A tab's name with its session count beside it.
 */
function TabLabel({ label, count }: TabLabelProps) {
  return (
    <>
      {label}
      {count === undefined ? null : <span className="text-muted-strong tabular-nums">{count}</span>}
    </>
  )
}

/**
 * The card's last line: how many rows show — of how many, while search or
 * kind narrows them — and in what order.
 */
function ListFooter({ shown, total, direction }: ListFooterProps) {
  return (
    <footer className="flex shrink-0 items-center justify-between gap-3 border-t border-border-soft px-[13px] py-1.5 font-mono text-mono text-muted-strong">
      <span>{shown === total ? sessionCount(total) : `${shown} of ${sessionCount(total)}`}</span>
      <span>last used · {directionLabels[direction]}</span>
    </footer>
  )
}
