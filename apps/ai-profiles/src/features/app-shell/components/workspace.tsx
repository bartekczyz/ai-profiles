import type { RightPane } from '@/features/app-shell/lib/shortcut-gates'
import type { AppId } from '@/lib/app-registry'
import type { SidebarEntry } from '@/lib/types'

import { Activity, Suspense, useRef } from 'react'

import { useShortcut } from '@/design'
import { ProfileDetail } from '@/features/profiles/components/profile-detail'
import { DefaultProfileDetail } from '@/features/profiles/components/profile-detail-default'
import { Sidebar } from '@/features/profiles/components/sidebar'
import { SettingsView } from '@/features/settings/components/settings-view'
import { SettingsViewSkeleton } from '@/features/settings/components/settings-view-skeleton'
import { QueryErrorBoundary } from '@/lib/query/error-boundary'

type SelectedEntryDetailProps = {
  /**
   * The selected sidebar entry, or `null`.
   */
  selected: SidebarEntry | null
  /**
   * Whether the detail pane's own shortcuts (⏎, ⌘C) are live.
   */
  shortcutsEnabled: boolean
  /**
   * Opens the edit dialog for the selected managed profile.
   */
  onEdit: () => void
  /**
   * Opens the delete dialog for the selected managed profile.
   */
  onDelete: () => void
  /**
   * Opens the import dialog for a default entry's app.
   */
  onMigrate: (app: AppId) => Promise<void>
}

/**
 * The detail pane for the selected entry: a managed profile's detail, a
 * default entry's detail, or nothing.
 */
function SelectedEntryDetail({ selected, shortcutsEnabled, onEdit, onDelete, onMigrate }: SelectedEntryDetailProps) {
  if (!selected) {
    return null
  }
  if (selected.kind === 'managed') {
    return (
      <QueryErrorBoundary>
        <ProfileDetail
          profile={selected.profile}
          shortcutsEnabled={shortcutsEnabled}
          onEdit={onEdit}
          onDelete={onDelete}
        />
      </QueryErrorBoundary>
    )
  }
  return (
    <QueryErrorBoundary>
      <DefaultProfileDetail
        entry={selected.entry}
        onMigrate={async () => {
          await onMigrate(selected.entry.app)
        }}
      />
    </QueryErrorBoundary>
  )
}

type WorkspaceProps = {
  /**
   * The sidebar entries, in display order.
   */
  entries: Array<SidebarEntry>
  /**
   * The selected entry's id, or `null`.
   */
  selectedId: string | null
  /**
   * The selected sidebar entry, or `null`.
   */
  selected: SidebarEntry | null
  /**
   * The view on the right-hand side.
   */
  rightPane: RightPane
  /**
   * Whether the detail pane's own shortcuts (⏎, ⌘C) are live.
   */
  detailShortcutsEnabled: boolean
  /**
   * Whether ⌘F focuses the sidebar filter (off under any overlay).
   */
  searchShortcutEnabled: boolean
  /**
   * Selects a sidebar entry and shows its detail.
   */
  onSelect: (id: string) => void
  /**
   * Opens the create profile dialog.
   */
  onCreate: () => void
  /**
   * Switches the right-hand view.
   */
  onRightPaneChange: (pane: RightPane) => void
  /**
   * Persists a new managed-profile order.
   */
  onReorder: (ids: Array<string>) => void
  /**
   * Opens the edit dialog for the selected managed profile.
   */
  onEdit: () => void
  /**
   * Opens the delete dialog for the selected managed profile.
   */
  onDelete: () => void
  /**
   * Opens the import dialog for an app.
   */
  onOpenMigration: (app: AppId) => Promise<void>
  /**
   * Opens the About dialog.
   */
  onOpenAbout: () => void
}

/**
 * The sidebar beside the right-hand pane: the selected entry's detail or
 * Settings.
 */
export function Workspace({
  entries,
  selectedId,
  selected,
  rightPane,
  detailShortcutsEnabled,
  searchShortcutEnabled,
  onSelect,
  onCreate,
  onRightPaneChange,
  onReorder,
  onEdit,
  onDelete,
  onOpenMigration,
  onOpenAbout,
}: WorkspaceProps) {
  const detailVisible = rightPane === 'profile' && selected !== null

  // ⌘F focuses the sidebar profile-filter input. Registered here so it only
  // exists while the sidebar is mounted (empty-state owns the whole window
  // with no sidebar).
  const searchInputRef = useRef<HTMLInputElement>(null)
  useShortcut(
    'focus-search',
    () => {
      searchInputRef.current?.focus()
      searchInputRef.current?.select()
    },
    { enabled: searchShortcutEnabled },
  )

  return (
    <div className="flex min-h-0 flex-1">
      <Sidebar
        entries={entries}
        selectedId={selectedId}
        searchInputRef={searchInputRef}
        onSelect={onSelect}
        onCreate={onCreate}
        onSettings={() => onRightPaneChange('settings')}
        onReorder={onReorder}
      />
      {/* Activity keeps the off-screen pane mounted so toggling gear ↔ profile
          never re-fetches dependencies/backups or re-runs profile-detail effects.
          ProfileDetail manages its own Suspense for the per-profile paths fetch
          — the header identity block renders with sidebar-provided data
          immediately. */}
      <Activity mode={detailVisible ? 'visible' : 'hidden'}>
        <SelectedEntryDetail
          selected={selected}
          shortcutsEnabled={detailShortcutsEnabled}
          onEdit={onEdit}
          onDelete={onDelete}
          onMigrate={onOpenMigration}
        />
      </Activity>
      <Activity mode={rightPane === 'settings' ? 'visible' : 'hidden'}>
        <Suspense fallback={<SettingsViewSkeleton />}>
          <QueryErrorBoundary>
            <SettingsView
              onClose={() => onRightPaneChange('profile')}
              onOpenMigration={(app) => {
                void onOpenMigration(app)
              }}
              onOpenAbout={onOpenAbout}
            />
          </QueryErrorBoundary>
        </Suspense>
      </Activity>
    </div>
  )
}
