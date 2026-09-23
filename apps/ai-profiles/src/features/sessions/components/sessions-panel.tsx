import type { AppId } from '@/lib/app-registry'
import type { Session, SessionAction } from '@/lib/types'
import type { KindFilter, SessionsTab, SortDirection } from '../lib/session-filters'

import { useState } from 'react'

import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/design/ui/tabs'
import { appSpecs } from '@/lib/app-registry'
import { extractErrorMessage } from '@/lib/extract-error-message'

import { useSessions } from '../api/use-sessions'
import { countByTab, filterSessions, hasBothKinds, sortSessions } from '../lib/session-filters'
import { ConfirmSessionActionDialog } from './confirm-session-action-dialog'
import { SessionsControls } from './sessions-controls'
import { SessionsList } from './sessions-list'
import { sessionsPanelClasses } from './sessions-panel-skeleton'

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

const tabContentClasses = 'flex min-h-0 flex-col'

const tabTriggerClasses = 'flex-none px-0.5 pb-1 text-[13px] tracking-[-0.005em] text-muted data-active:text-ink'

/**
 * The sessions a profile owns, beside (or, in a narrow pane, below) its
 * details.
 *
 * Active and Archived tabs head the panel; under them, one row of controls —
 * search, the Desktop/CLI filter and the last-used sort — applies to whichever
 * tab is open. In the two-column layout the panel fills the column's height
 * and only the rows scroll; stacked, it grows with its rows and the pane
 * scrolls as a whole.
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

  const listed = sessionsQuery.data?.sessions
  const sessions = listed ?? []
  const counts = countByTab(sessions)
  const inTab = filterSessions(sessions, { tab, kind: 'all', query: '' })
  const kindFilterShown = hasBothKinds(inTab)
  const kind = kindFilterShown ? kindChoice : 'all'
  const visible = sortSessions(filterSessions(inTab, { tab, kind, query }), direction)
  // A failed refetch keeps the rows it already has; only a listing that never
  // landed shows the failure.
  const errorMessage = listed === undefined && sessionsQuery.isError ? extractErrorMessage(sessionsQuery.error) : null

  const list = (
    <SessionsList
      loading={sessionsQuery.isPending}
      retrying={sessionsQuery.isFetching}
      errorMessage={errorMessage}
      tabTotal={inTab.length}
      sessions={visible}
      emptyTitle={tab === 'active' ? 'No sessions yet' : 'No archived sessions'}
      emptyHint={
        tab === 'active' ? `${appSpecs[app].cliDisplayName} sessions this profile starts show up here.` : undefined
      }
      onRetry={() => {
        void sessionsQuery.refetch()
      }}
      onClearSearch={() => setQuery('')}
      onAction={(session, action) => setPendingAction({ session, action })}
    />
  )

  return (
    <Tabs
      value={tab}
      className={sessionsPanelClasses}
      onValueChange={(value) => setTab(value === 'archived' ? 'archived' : 'active')}
    >
      <TabsList variant="line" aria-label="Sessions" className="h-auto gap-4 p-0">
        <TabsTrigger value="active" className={tabTriggerClasses}>
          <TabLabel label="Active" count={listed === undefined ? undefined : counts.active} />
        </TabsTrigger>
        <TabsTrigger value="archived" className={tabTriggerClasses}>
          <TabLabel label="Archived" count={listed === undefined ? undefined : counts.archived} />
        </TabsTrigger>
      </TabsList>
      <SessionsControls
        kindFilterShown={kindFilterShown}
        query={query}
        kind={kind}
        direction={direction}
        onQueryChange={setQuery}
        onKindChange={setKindChoice}
        onDirectionChange={setDirection}
      />
      <TabsContent value="active" className={tabContentClasses}>
        {list}
      </TabsContent>
      <TabsContent value="archived" className={tabContentClasses}>
        {list}
      </TabsContent>
      {pendingAction === null ? null : (
        <ConfirmSessionActionDialog
          profileId={profileId}
          session={pendingAction.session}
          action={pendingAction.action}
          onClose={() => setPendingAction(null)}
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
      {count === undefined ? null : (
        <span className="font-mono text-[11px] font-normal text-muted-strong tabular-nums">{count}</span>
      )}
    </>
  )
}
