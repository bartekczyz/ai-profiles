// SPDX-License-Identifier: MIT

import type { RightPane } from '@/features/app-shell/lib/shortcut-gates'

import { Suspense, useState } from 'react'

import { AboutDialog } from '@/features/about/components/about-dialog'
import { ProfileIndexHotkeys } from '@/features/app-shell/components/profile-index-hotkeys'
import { TitleBarDragStrip } from '@/features/app-shell/components/title-bar-drag-strip'
import { Workspace } from '@/features/app-shell/components/workspace'
import { shortcutGates } from '@/features/app-shell/lib/shortcut-gates'
import { useAppShortcuts } from '@/features/app-shell/use-app-shortcuts'
import { useWindowIntegration } from '@/features/app-shell/use-window-integration'
import { CommandPalette } from '@/features/command-palette/components/command-palette'
import { useCommandPalette } from '@/features/command-palette/use-command-palette'
import { useDependencies } from '@/features/dependencies/api/use-dependencies'
import { useMigrationLauncher } from '@/features/migration/api/use-migration-launcher'
import { MigrationDialogHost } from '@/features/migration/components/migration-dialog-host'
import { PathSetupBannerHost } from '@/features/onboarding/components/path-setup-banner-host'
import { WelcomeDialog } from '@/features/onboarding/components/welcome-dialog'
import { useProfileLastUsed } from '@/features/profiles/api/use-profile-last-used'
import { useProfiles } from '@/features/profiles/api/use-profiles'
import { resolveSelection, useSidebarEntries } from '@/features/profiles/api/use-sidebar-entries'
import { useSidebarSelection } from '@/features/profiles/api/use-sidebar-selection'
import { EmptyStateScreen } from '@/features/profiles/components/empty-state-screen'
import { ProfileDetailSkeleton } from '@/features/profiles/components/profile-detail-skeleton'
import { ProfileDialogs } from '@/features/profiles/components/profile-dialogs'
import { SidebarSkeleton } from '@/features/profiles/components/sidebar-skeleton'
import { UpdateToastTrigger } from '@/features/updater/components/update-toast-trigger'
import { WhatsNewHost } from '@/features/whats-new/components/whats-new-host'
import { wrapperCommand } from '@/lib/app-registry'
import { useAppState } from '@/lib/app-state/use-app-state'
import { QueryErrorBoundary } from '@/lib/query/error-boundary'

type DialogState =
  | { kind: 'none' }
  | { kind: 'create' }
  | { kind: 'edit' }
  | { kind: 'delete' }
  | { kind: 'about' }
  | { kind: 'whats-new' }

function AppShellSkeleton() {
  return (
    <div className="flex h-full">
      <SidebarSkeleton />
      <ProfileDetailSkeleton />
    </div>
  )
}

function AppContent() {
  const profiles = useProfiles()
  const entries = useSidebarEntries()
  const selection = useSidebarSelection(entries)
  const migration = useMigrationLauncher()
  const appState = useAppState()
  const dependencies = useDependencies()
  const lastUsed = useProfileLastUsed()
  const palette = useCommandPalette()
  const [dialog, setDialog] = useState<DialogState>({ kind: 'none' })
  const [rightPane, setRightPane] = useState<RightPane>('profile')

  const { selected, managedSelected } = resolveSelection(entries, selection.selectedId)
  useWindowIntegration({
    themeMode: appState.state.themeMode,
    selected,
    onOpenAbout: () => setDialog({ kind: 'about' }),
  })

  const gates = shortcutGates({
    dialogOpen: dialog.kind !== 'none',
    paletteOpen: palette.open,
    migrationOpen: migration.app !== null,
    rightPane,
    hasSelection: selected !== null,
    hasManagedSelection: managedSelected !== null,
  })

  useAppShortcuts({
    gates,
    onTogglePalette: palette.toggle,
    onCreate: requestCreateProfile,
    onToggleSettings: () => setRightPane((current) => (current === 'settings' ? 'profile' : 'settings')),
    onDetectImport: migration.openFirstImportable,
    // Gated on a managed selection (`gates.manageSelected`) — default rows
    // don't support edit/delete.
    onEditSelected: () => setDialog({ kind: 'edit' }),
    onDeleteSelected: () => setDialog({ kind: 'delete' }),
  })

  function requestCreateProfile() {
    setDialog({ kind: 'create' })
  }

  function selectEntry(id: string) {
    selection.select(id)
    setRightPane('profile')
  }

  function closeDialog() {
    setDialog({ kind: 'none' })
  }

  if (!appState.state.welcomeShown) {
    return (
      <WelcomeDialog
        open
        onContinue={async () => {
          await appState.update({ welcomeShown: true })
        }}
      />
    )
  }

  return (
    <div className="relative flex h-full flex-col">
      <TitleBarDragStrip />
      <UpdateToastTrigger />
      <PathSetupBannerHost />
      {/* The empty-state screen owns the whole window when there are no entries
          yet — no sidebar, no panes. As soon as the first profile/default entry
          lands, the sidebar appears and the detail pane takes over. */}
      {entries.length === 0 ? (
        <EmptyStateScreen
          dependencies={dependencies.deps}
          onCreate={requestCreateProfile}
          onRefresh={dependencies.refresh}
        />
      ) : (
        <Workspace
          entries={entries}
          selectedId={selection.selectedId}
          selected={selected}
          rightPane={rightPane}
          detailShortcutsEnabled={gates.detail}
          searchShortcutEnabled={gates.global}
          onSelect={selectEntry}
          onCreate={requestCreateProfile}
          onRightPaneChange={setRightPane}
          onReorder={(ids) => {
            void profiles.reorder(ids)
          }}
          onEdit={() => setDialog({ kind: 'edit' })}
          onDelete={() => setDialog({ kind: 'delete' })}
          onOpenMigration={migration.open}
          onOpenAbout={() => setDialog({ kind: 'about' })}
        />
      )}

      <ProfileDialogs
        createOpen={dialog.kind === 'create'}
        editOpen={dialog.kind === 'edit'}
        deleteOpen={dialog.kind === 'delete'}
        profile={managedSelected}
        onClose={closeDialog}
        onCreated={selection.select}
      />

      <Suspense fallback={null}>
        <AboutDialog
          open={dialog.kind === 'about'}
          onClose={closeDialog}
          onOpenWhatsNew={() => setDialog({ kind: 'whats-new' })}
        />
        <WhatsNewHost
          open={dialog.kind === 'whats-new'}
          onOpen={() => setDialog({ kind: 'whats-new' })}
          onClose={closeDialog}
        />
      </Suspense>

      <MigrationDialogHost
        launcher={migration}
        onImported={async (imported) => {
          await profiles.refresh()
          selectEntry(imported.id)
        }}
      />

      {/* Disabled when any overlay is open to avoid stealing keystrokes from
          the dialog/palette/migration prompt. */}
      <ProfileIndexHotkeys entries={entries} enabled={gates.global} onSelect={selectEntry} />

      <CommandPalette
        open={palette.open}
        entries={entries}
        selectedId={selection.selectedId}
        importableApps={migration.importableApps}
        onClose={palette.close}
        onSwitch={selectEntry}
        onLaunch={(profileId) => {
          void lastUsed.launchDesktop(profileId)
        }}
        onCopy={(profile) => {
          void lastUsed.copyCli({ profileId: profile.id, command: wrapperCommand(profile.app, profile.slug) })
        }}
        onCreate={requestCreateProfile}
        onSettings={() => setRightPane('settings')}
        onImport={(app) => {
          void migration.open(app)
        }}
      />
    </div>
  )
}

export default function App() {
  return (
    <QueryErrorBoundary>
      <Suspense fallback={<AppShellSkeleton />}>
        <AppContent />
      </Suspense>
    </QueryErrorBoundary>
  )
}
