import type { AppId } from '@/lib/app-registry'
import type { Session, SessionAction, Spend } from '@/lib/types'
import type { CostSum } from '../lib/session-cost'
import type { KindFilter, SessionSort, SessionsTab } from '../lib/session-filters'
import type { MoveTarget } from './move-session-dialog'

import { Suspense, useState } from 'react'

import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/design/ui/tabs'
import { useProfileUsageSnapshot } from '@/features/profiles/api/use-profile-usage'
import { appFromEntry, entryId, useSidebarEntries } from '@/features/profiles/api/use-sidebar-entries'
import { appSpecs } from '@/lib/app-registry'
import { formatMoney, formatUsd } from '@/lib/format-cost'

import { useSessionCosts } from '../api/use-session-costs'
import { useSessions } from '../api/use-sessions'
import { sessionCount } from '../lib/session-count'
import { emptyTabCopy, sessionsView } from '../lib/sessions-view'
import { ConfirmSessionActionDialog } from './confirm-session-action-dialog'
import { MoveSessionDialog } from './move-session-dialog'
import { RefreshFailedNote } from './refresh-failed-note'
import { RepairBanner } from './repair-banner'
import { SessionsControls, sortChoices } from './sessions-controls'
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

/**
 * Where a profile's sessions can move, and what the profile is called.
 */
type MoveTargets = {
  /**
   * The other profiles of the app a session can be moved to.
   */
  moveTargets: Array<MoveTarget>
  /**
   * The profile's name.
   */
  profileLabel: string
}

type SessionsHeaderProps = {
  /**
   * How many sessions each tab holds; absent until the first listing lands.
   */
  counts?: Record<SessionsTab, number>
}

type TabNoticesProps = {
  /**
   * Whether the repair offer may show — on the Active tab only.
   */
  repairShown: boolean
  /**
   * Whether a refresh failed over the rows on screen.
   */
  refreshFailed: boolean
  /**
   * Whether a Retry is under way.
   */
  retrying: boolean
  /**
   * The profile whose sessions are listed.
   */
  profileId: string
  /**
   * The profile's name.
   */
  profileLabel: string
  /**
   * The sessions that need repair.
   */
  repairSessionIds: Array<string>
  /**
   * Refetches.
   */
  onRetry: () => void
}

type PendingDialogsProps = {
  /**
   * The profile whose sessions are listed.
   */
  profileId: string
  /**
   * The action waiting on confirmation, if any.
   */
  pendingAction: PendingAction | null
  /**
   * The move waiting on confirmation, if any.
   */
  pendingMove: PendingMove | null
  /**
   * Drops the pending action.
   */
  onCloseAction: () => void
  /**
   * Drops the pending move.
   */
  onCloseMove: () => void
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
   * The order of the rows.
   */
  sort: SessionSort
  /**
   * What the rows add up to at API prices; `null` without costs to add.
   */
  costSum: CostSum | null
  /**
   * What the profile paid in usage credits this month, as the usage card last
   * read it; none when it has no usage credits.
   */
  paid?: Spend
}

/**
 * A tab's body: a column that can shrink, so only its rows scroll.
 */
const tabContentClasses = 'flex min-h-0 flex-col'

/**
 * The Active / Archived tabs: a macOS-style segmented control — a recessed
 * track with the selected segment raised out of it.
 */
const tabListClasses =
  'h-7 gap-0.5 rounded-lg bg-black/[0.05] p-0.5 group-data-horizontal/tabs:h-7 dark:bg-white/[0.06]'

/**
 * An Active or Archived tab. No hover change, as on a native segmented
 * control; only the selected segment stands out.
 */
const tabTriggerClasses =
  'h-6 flex-none cursor-default gap-1.5 rounded-md border-0 px-3 text-[12px] font-medium text-muted data-active:bg-cream data-active:text-ink data-active:shadow-[0_1px_2px_rgba(0,0,0,0.1),0_0_0_0.5px_rgba(0,0,0,0.06)] dark:data-active:bg-white/[0.12] dark:data-active:shadow-none'

/**
 * The sessions a profile owns, beside (or, in a narrow pane, below) its
 * details, in a card that speaks the Usage card's language.
 *
 * A SESSIONS eyebrow heads the card with the Active and Archived tabs across
 * from it; under them, one toolbar — search, the Desktop/CLI filter and the
 * sort, by last use or API-equivalent cost — applies to whichever tab is open,
 * and a footer says how many rows show, what they add up to at API prices
 * beside what the profile paid in usage credits this month, and in what
 * order. Each row's API-equivalent cost is read apart from the list and fills
 * its column once read. The panel fills the height the pane gives it — the column
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
  const [sort, setSort] = useState<SessionSort>({ key: 'lastUsed', direction: 'desc' })
  const costsQuery = useSessionCosts(profileId)
  const paid = useProfileUsageSnapshot(profileId).data?.quota?.spend
  const [pendingAction, setPendingAction] = useState<PendingAction | null>(null)
  const [pendingMove, setPendingMove] = useState<PendingMove | null>(null)
  const { moveTargets, profileLabel } = useMoveTargets(profileId, app)
  const view = sessionsView(
    {
      sessions: sessionsQuery.data?.sessions,
      isError: sessionsQuery.isError,
      error: sessionsQuery.error,
      costs: costsQuery.data?.sessions,
    },
    { tab, kindChoice, query, sort },
  )
  const empty = emptyTabCopy(tab, app)
  const retry = () => {
    void sessionsQuery.refetch()
  }

  return (
    <Tabs
      value={tab}
      className={sessionsPanelClasses}
      onValueChange={(value) => setTab(value === 'archived' ? 'archived' : 'active')}
    >
      <SessionsHeader counts={view.counts} />
      <SessionsControls
        kindFilterShown={view.kindFilterShown}
        query={query}
        kind={view.kind}
        sort={sort}
        onQueryChange={setQuery}
        onKindChange={setKindChoice}
        onSortChange={setSort}
      />
      {/* One content slot, for whichever tab is open: the rows below the
          controls are the same list either way, filtered by the tab. Keyed
          by the tab, so switching starts it afresh as separate tabs would. */}
      <TabsContent key={tab} value={tab} className={tabContentClasses}>
        <TabNotices
          repairShown={tab === 'active'}
          refreshFailed={view.refreshFailed}
          retrying={sessionsQuery.isFetching}
          profileId={profileId}
          profileLabel={profileLabel}
          repairSessionIds={view.repairSessionIds}
          onRetry={retry}
        />
        <SessionsList
          loading={sessionsQuery.isPending}
          retrying={sessionsQuery.isFetching}
          failure={view.failure}
          tabTotal={view.tabTotal}
          sessions={view.visible}
          app={app}
          moveTargets={moveTargets}
          costs={{ pending: costsQuery.isPending, byId: costsQuery.data?.sessions }}
          emptyTitle={empty.title}
          emptyHint={empty.hint}
          onRetry={retry}
          onClearSearch={() => setQuery('')}
          onAction={(session, action) => setPendingAction({ session, action })}
          onMove={(session, destination) => setPendingMove({ session, destination })}
        />
      </TabsContent>
      {view.footerShown ? (
        <ListFooter shown={view.visible.length} total={view.tabTotal} sort={sort} costSum={view.costSum} paid={paid} />
      ) : null}
      <PendingDialogs
        profileId={profileId}
        pendingAction={pendingAction}
        pendingMove={pendingMove}
        onCloseAction={() => setPendingAction(null)}
        onCloseMove={() => setPendingMove(null)}
      />
    </Tabs>
  )
}

/**
 * The other profiles of `app` a session of `profileId` can move to, and what
 * the profile itself is called — its app's name for a stock install the
 * sidebar doesn't list.
 */
function useMoveTargets(profileId: string, app: AppId): MoveTargets {
  const profiles: Array<MoveTarget> = useSidebarEntries()
    .filter((entry) => appFromEntry(entry) === app)
    .map((entry) => ({
      id: entryId(entry),
      label: entry.kind === 'managed' ? entry.profile.name : entry.entry.name,
    }))
  return {
    moveTargets: profiles.filter((profile) => profile.id !== profileId),
    profileLabel: profiles.find((profile) => profile.id === profileId)?.label ?? appSpecs[app].displayName,
  }
}

/**
 * The card's eyebrow row: the SESSIONS eyebrow, with the Active and Archived
 * tabs across from it.
 */
function SessionsHeader({ counts }: SessionsHeaderProps) {
  return (
    <header className={sessionsHeaderClasses}>
      <h2 className="text-[12.5px] font-medium text-ink-soft">Sessions</h2>
      <TabsList aria-label="Sessions" className={tabListClasses}>
        <TabsTrigger value="active" className={tabTriggerClasses}>
          <TabLabel label="Active" count={counts?.active} />
        </TabsTrigger>
        <TabsTrigger value="archived" className={tabTriggerClasses}>
          <TabLabel label="Archived" count={counts?.archived} />
        </TabsTrigger>
      </TabsList>
    </header>
  )
}

/**
 * What heads the open tab's rows: the offer to repair sessions, on the Active
 * tab, and a note when a refresh failed.
 */
function TabNotices({
  repairShown,
  refreshFailed,
  retrying,
  profileId,
  profileLabel,
  repairSessionIds,
  onRetry,
}: TabNoticesProps) {
  return (
    <>
      {/* Its own boundary: the banner waits on the app state for what was
          dismissed, and the rows shouldn't wait with it. */}
      {repairShown ? (
        <Suspense fallback={null}>
          <RepairBanner profileId={profileId} profileLabel={profileLabel} repairSessionIds={repairSessionIds} />
        </Suspense>
      ) : null}
      {refreshFailed ? <RefreshFailedNote retrying={retrying} onRetry={onRetry} /> : null}
    </>
  )
}

/**
 * The confirm dialogs for an action or a move asked for from a row.
 */
function PendingDialogs({ profileId, pendingAction, pendingMove, onCloseAction, onCloseMove }: PendingDialogsProps) {
  return (
    <>
      {pendingAction === null ? null : (
        <ConfirmSessionActionDialog
          profileId={profileId}
          session={pendingAction.session}
          action={pendingAction.action}
          onClose={onCloseAction}
        />
      )}
      {pendingMove === null ? null : (
        <MoveSessionDialog
          profileId={profileId}
          session={pendingMove.session}
          destination={pendingMove.destination}
          onClose={onCloseMove}
        />
      )}
    </>
  )
}

/**
 * A tab's name with its session count beside it.
 */
function TabLabel({ label, count }: TabLabelProps) {
  return (
    <>
      {label}
      {count === undefined ? null : <span className="font-normal text-muted-strong tabular-nums">{count}</span>}
    </>
  )
}

/**
 * The card's last line: how many rows show — of how many, while search or
 * kind narrows them — and what they add up to at API prices, with a `+` when
 * some of their use couldn't be priced; then in what order they show.
 *
 * That sum is an API-equivalent, whether a plan covered the use or it was
 * paid for, so what the profile actually paid in usage credits this month
 * follows it, as the usage card last read it.
 */
function ListFooter({ shown, total, sort, costSum, paid }: ListFooterProps) {
  const choice = sortChoices.find((each) => each.key === sort.key) ?? sortChoices[0]
  return (
    <footer className="flex shrink-0 items-center justify-between gap-3 border-t border-border-soft px-[13px] py-1.5 font-mono text-mono text-muted-strong">
      <span className="min-w-0 truncate">
        {shown === total ? sessionCount(total) : `${shown} of ${sessionCount(total)}`}
        {costSum === null ? null : (
          <>
            <span className="mx-1.5 text-border">·</span>
            <span className="text-ink-soft tabular-nums">
              {formatUsd(costSum.usd)}
              {costSum.partial ? '+' : ''}
            </span>{' '}
            API-equivalent
          </>
        )}
        {costSum === null || paid === undefined ? null : (
          <>
            <span className="mx-1.5 text-border">·</span>
            <span className="text-ink-soft tabular-nums">
              {formatMoney(paid.usedMinor, paid.currency, paid.exponent)}
            </span>{' '}
            paid in usage credits this month
          </>
        )}
      </span>
      <span className="shrink-0">
        {choice.label.toLowerCase()} · {choice.directions[sort.direction].toLowerCase()}
      </span>
    </footer>
  )
}
