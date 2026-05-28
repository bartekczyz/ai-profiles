// SPDX-License-Identifier: MIT

import type { SidebarEntry } from '@/lib/types'

import { Activity, Suspense, useEffect, useRef, useState } from 'react'

import { useHotkey } from '@tanstack/react-hotkeys'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'

import { useShortcut, useTheme } from '@/design'
import { AboutDialog } from '@/features/about/components/about-dialog'
import { CommandPalette } from '@/features/command-palette/components/command-palette'
import { useCommandPalette } from '@/features/command-palette/use-command-palette'
import { useDependencies } from '@/features/dependencies/api/use-dependencies'
import { useMigration } from '@/features/migration/api/use-migration'
import { MigrationDialog } from '@/features/migration/components/migration-dialog'
import { PathSetupBanner } from '@/features/onboarding/components/path-setup-banner'
import { WelcomeDialog } from '@/features/onboarding/components/welcome-dialog'
import { useProfileLastUsed } from '@/features/profiles/api/use-profile-last-used'
import { useProfiles } from '@/features/profiles/api/use-profiles'
import { entryId, useSidebarEntries } from '@/features/profiles/api/use-sidebar-entries'
import { useSidebarSelection } from '@/features/profiles/api/use-sidebar-selection'
import { CreateProfileDialog } from '@/features/profiles/components/create-profile-dialog'
import { DeleteProfileDialog } from '@/features/profiles/components/delete-profile-dialog'
import { EditProfileDialog } from '@/features/profiles/components/edit-profile-dialog'
import { EmptyStateScreen } from '@/features/profiles/components/empty-state-screen'
import { ProfileDetail } from '@/features/profiles/components/profile-detail'
import { DefaultProfileDetail } from '@/features/profiles/components/profile-detail-default'
import { ProfileDetailSkeleton } from '@/features/profiles/components/profile-detail-skeleton'
import { Sidebar } from '@/features/profiles/components/sidebar'
import { SidebarSkeleton } from '@/features/profiles/components/sidebar-skeleton'
import { SettingsView } from '@/features/settings/components/settings-view'
import { SettingsViewSkeleton } from '@/features/settings/components/settings-view-skeleton'
import { UpdateToastTrigger } from '@/features/updater/components/update-toast-trigger'
import { useAppState } from '@/lib/app-state/use-app-state'
import { QueryErrorBoundary } from '@/lib/query/error-boundary'

type DialogState = { kind: 'none' } | { kind: 'create' } | { kind: 'edit' } | { kind: 'delete' } | { kind: 'about' }

type RightPane = { kind: 'profile' } | { kind: 'settings' }

const SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000
const DISMISSAL_WINDOW_MS = 7 * SEVEN_DAYS_MS

function isWithinDismissalWindow(timestamp: string | null | undefined): boolean {
  if (!timestamp) {
    return false
  }
  return Date.now() - new Date(timestamp).getTime() < DISMISSAL_WINDOW_MS
}

function AppShellSkeleton() {
  return (
    <div className="flex h-full">
      <SidebarSkeleton />
      <ProfileDetailSkeleton />
    </div>
  )
}

const profileIndexKeys = ['Mod+1', 'Mod+2', 'Mod+3', 'Mod+4', 'Mod+5', 'Mod+6', 'Mod+7', 'Mod+8', 'Mod+9'] as const

type SelectByIndexHotkeyProps = {
  index: number
  enabled: boolean
  onSelect: (index: number) => void
}

/**
 * One Mod+N binding per profile slot. Each instance registers a single
 * hotkey — kept as a child component so we can map over indices without
 * violating the rules-of-hooks ban on conditional/looped hook calls.
 * The discrete `profileIndexKeys` tuple keeps the keys narrowly typed
 * (`Mod+${number}` is too broad for the library's Hotkey union).
 */
function SelectByIndexHotkey({ index, enabled, onSelect }: SelectByIndexHotkeyProps) {
  useHotkey(
    profileIndexKeys[index],
    () => {
      onSelect(index)
    },
    { enabled },
  )
  return null
}

function AppContent() {
  const profiles = useProfiles()
  const entries = useSidebarEntries()
  const selection = useSidebarSelection(entries)
  const migration = useMigration()
  const appState = useAppState()
  const dependencies = useDependencies()
  const lastUsed = useProfileLastUsed()
  const palette = useCommandPalette()
  const [dialog, setDialog] = useState<DialogState>({ kind: 'none' })
  const [submitting, setSubmitting] = useState(false)
  const [rightPane, setRightPane] = useState<RightPane>({ kind: 'profile' })
  const [forceMigrationOpen, setForceMigrationOpen] = useState(false)

  const theme = useTheme()
  const persistedThemeMode = appState.state.themeMode

  useEffect(() => {
    if (persistedThemeMode !== theme.mode) {
      theme.setMode(persistedThemeMode)
    }
  }, [persistedThemeMode, theme])

  // Suppress the system context menu app-wide in production builds — this is
  // a Tauri window, not a browser. Inputs and `[data-selectable=true]`
  // regions opt back in so a user can still right-click to paste into the
  // profile name field, etc.
  //
  // In dev (`pnpm tauri dev` / `vite dev`) we leave the context menu alone so
  // the webview's "Inspect Element" stays accessible while iterating.
  useEffect(() => {
    if (import.meta.env.DEV) {
      return
    }
    function handleContextMenu(event: MouseEvent) {
      const target = event.target as HTMLElement | null
      if (!target) {
        event.preventDefault()
        return
      }
      if (target.closest('input, textarea, [contenteditable="true"], [data-selectable="true"]')) {
        return
      }
      event.preventDefault()
    }
    window.addEventListener('contextmenu', handleContextMenu)
    return () => window.removeEventListener('contextmenu', handleContextMenu)
  }, [])

  // Bridge the macOS App menu's "About claude-profiles" item to our custom
  // dialog. The menu item (set up in src-tauri/src/lib.rs) emits the
  // `open-about` event; this listener catches it and opens the dialog.
  // Replaces the tiny native About panel macOS would otherwise show.
  useEffect(() => {
    const unlistenPromise = listen('open-about', () => {
      setDialog({ kind: 'about' })
    })
    return () => {
      void unlistenPromise.then((unlisten) => {
        unlisten()
      })
    }
  }, [])

  // Native (not React-synthetic) mousedown listener on the title-bar drag
  // strip. React's synthetic events fire AFTER the browser has finished
  // delivering the native event, by which point macOS has already decided
  // what to do with the click — when the window is active that means
  // `startDragging()` becomes a no-op. Binding the listener directly to
  // the element via `addEventListener` (the pattern Tauri's own docs use)
  // intercepts the event early enough that drag works in both
  // active and inactive window states.
  const dragStripRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const node = dragStripRef.current
    if (!node) {
      return
    }
    function handleMouseDown(event: MouseEvent) {
      if (event.buttons !== 1) {
        return
      }
      const appWindow = getCurrentWindow()
      if (event.detail === 2) {
        void appWindow.toggleMaximize()
      } else {
        void appWindow.startDragging()
      }
    }
    node.addEventListener('mousedown', handleMouseDown)
    return () => node.removeEventListener('mousedown', handleMouseDown)
  }, [])

  const selected = entries.find((entry) => entryId(entry) === selection.selectedId) ?? null
  const managedSelected = selected?.kind === 'managed' ? selected.profile : null

  const shouldShowWelcome = !appState.state.welcomeShown

  const showMigration = forceMigrationOpen

  const anyCliProfile = profiles.profiles.some((profile) => profile.surfaces.cli)
  const pathBannerDismissedRecently = isWithinDismissalWindow(appState.state.pathBannerDismissedAt)

  const shouldShowPathBanner =
    appState.state.welcomeShown &&
    dependencies.deps.localBinOnPath === false &&
    anyCliProfile &&
    !pathBannerDismissedRecently

  const dialogOpen = dialog.kind !== 'none'
  const overlayOpen = dialogOpen || palette.open || showMigration
  const detailEnabled = rightPane.kind === 'profile' && selected !== null && !overlayOpen

  // Global shortcuts — suppressed when a blocking overlay (dialog,
  // palette, migration prompt) is on top, EXCEPT toggle-palette which
  // must keep working while the palette itself is open so ⌘K closes it.
  useShortcut('toggle-palette', palette.toggle, { enabled: !dialogOpen && !showMigration })
  useShortcut('open-create-profile', requestCreateProfile, { enabled: !overlayOpen })
  useShortcut(
    'toggle-settings',
    () => setRightPane((current) => (current.kind === 'settings' ? { kind: 'profile' } : { kind: 'settings' })),
    { enabled: !overlayOpen },
  )
  useShortcut(
    'open-detect-import',
    () => {
      void migration.refresh().then(() => setForceMigrationOpen(true))
    },
    { enabled: !overlayOpen },
  )

  // ⌘F focuses the sidebar profile-filter input. Gated on the sidebar
  // being mounted (empty-state owns the whole window with no sidebar)
  // and no overlay being on top.
  const searchInputRef = useRef<HTMLInputElement>(null)
  const sidebarVisible = entries.length > 0
  useShortcut(
    'focus-search',
    () => {
      searchInputRef.current?.focus()
      searchInputRef.current?.select()
    },
    { enabled: !overlayOpen && sidebarVisible },
  )

  // Detail-scope shortcuts — gated on a managed profile being selected, the
  // detail pane being on top, and no overlay (dialog/palette/migration)
  // covering it. Default rows don't support edit/delete/launch/copy.
  useShortcut(
    'edit-selected',
    () => {
      if (managedSelected) {
        setDialog({ kind: 'edit' })
      }
    },
    { enabled: detailEnabled && managedSelected !== null },
  )
  useShortcut(
    'delete-selected',
    () => {
      if (managedSelected) {
        setDialog({ kind: 'delete' })
      }
    },
    { enabled: detailEnabled && managedSelected !== null },
  )
  useShortcut(
    'open-selected-desktop',
    () => {
      if (managedSelected?.surfaces.gui) {
        void lastUsed.launchDesktop(managedSelected.id)
      }
    },
    { enabled: detailEnabled && managedSelected !== null },
  )
  useShortcut(
    'copy-selected-cli',
    () => {
      if (managedSelected?.surfaces.cli) {
        void lastUsed.copyCli({ profileId: managedSelected.id, command: `claude-${managedSelected.slug}` })
      }
    },
    { enabled: detailEnabled && managedSelected !== null },
  )

  async function handleCreate(input: Parameters<typeof profiles.create>[0]) {
    setSubmitting(true)
    try {
      const created = await profiles.create(input)
      selection.select(created.id)
    } finally {
      setSubmitting(false)
    }
  }

  async function handleEdit(input: { name: string; color: string; surfaces: { gui: boolean; cli: boolean } }) {
    if (!managedSelected) {
      return
    }
    setSubmitting(true)
    try {
      const nameChanged = input.name !== managedSelected.name
      const colorChanged = input.color.toLowerCase() !== managedSelected.color.toLowerCase()
      if (nameChanged || colorChanged) {
        await profiles.update({
          id: managedSelected.id,
          patch: { name: input.name, color: input.color },
        })
      }
      if (input.surfaces.gui !== managedSelected.surfaces.gui) {
        await profiles.toggle({ id: managedSelected.id, surface: 'gui', enabled: input.surfaces.gui })
      }
      if (input.surfaces.cli !== managedSelected.surfaces.cli) {
        await profiles.toggle({ id: managedSelected.id, surface: 'cli', enabled: input.surfaces.cli })
      }
    } finally {
      setSubmitting(false)
    }
  }

  async function handleDelete(input: { moveToTrash: boolean }) {
    if (!managedSelected) {
      return
    }
    await profiles.remove({ id: managedSelected.id, ...input })
  }

  function requestCreateProfile() {
    setDialog({ kind: 'create' })
  }

  if (shouldShowWelcome) {
    return (
      <WelcomeDialog
        open
        onContinue={async () => {
          await appState.update({ welcomeShown: true })
        }}
      />
    )
  }

  // The empty-state screen owns the whole window when there are no entries
  // yet — no sidebar, no panes. As soon as the first profile/default entry
  // lands, the sidebar appears and the detail pane takes over.
  const isEmpty = entries.length === 0

  return (
    <div className="relative flex h-full flex-col">
      {/* Tauri overlay-style title bar dragger.
          `titleBarStyle: Overlay` in tauri.conf.json drops the system
          title bar so the traffic-light area becomes empty content space.
          Without an explicit drag handler the user can't move the window
          (the traffic lights themselves are buttons, not draggers). This
          28px-tall transparent strip explicitly calls Tauri's
          `startDragging()` on mousedown — the docs' recommended approach
          for SPA-rendered drag regions, more reliable than relying on the
          attribute-based auto-binding which often misses React-rendered
          nodes. Double-click toggles maximize, matching native macOS
          behaviour.
          - z-10 keeps it below dialogs (z-40/50) so overlay surfaces stay
            interactive.
          - It sits over the sidebar's `pt-11` empty area and the right
            pane's `pt-10`/`py-9` top padding, so no interactive content
            is obscured. */}
      <div ref={dragStripRef} aria-hidden className="absolute inset-x-0 top-0 z-10 h-7" />
      <UpdateToastTrigger />
      {shouldShowPathBanner ? (
        <PathSetupBanner
          onFixed={async () => {
            await dependencies.refresh()
          }}
          onDismiss={async () => {
            await appState.update({ pathBannerDismissedAt: new Date().toISOString() })
          }}
        />
      ) : null}
      {isEmpty ? (
        <EmptyStateScreen
          dependencies={dependencies.deps}
          onCreate={requestCreateProfile}
          onRefresh={dependencies.refresh}
        />
      ) : (
        <div className="flex min-h-0 flex-1">
          <Sidebar
            entries={entries}
            selectedId={selection.selectedId}
            searchInputRef={searchInputRef}
            onSelect={(id) => {
              selection.select(id)
              setRightPane({ kind: 'profile' })
            }}
            onCreate={requestCreateProfile}
            onSettings={() => setRightPane({ kind: 'settings' })}
            onReorder={(ids) => {
              void profiles.reorder(ids)
            }}
          />
          {/* Activity keeps the off-screen pane mounted so toggling gear ↔ profile
              never re-fetches dependencies/backups or re-runs profile-detail effects.
              ProfileDetail manages its own Suspense for the per-profile paths fetch
              — the header, danger link, and hint strip render with sidebar-provided
              data immediately. */}
          <Activity mode={rightPane.kind === 'profile' && selected !== null ? 'visible' : 'hidden'}>
            {selected?.kind === 'managed' ? (
              <QueryErrorBoundary>
                <ProfileDetail
                  profile={selected.profile}
                  onEdit={() => setDialog({ kind: 'edit' })}
                  onDelete={() => setDialog({ kind: 'delete' })}
                  onToggle={async (surface, enabled) => {
                    await profiles.toggle({ id: selected.profile.id, surface, enabled })
                  }}
                />
              </QueryErrorBoundary>
            ) : selected?.kind === 'default' ? (
              <QueryErrorBoundary>
                <DefaultProfileDetail
                  entry={selected.entry}
                  onMigrate={async () => {
                    await migration.refresh()
                    setForceMigrationOpen(true)
                  }}
                />
              </QueryErrorBoundary>
            ) : null}
          </Activity>
          <Activity mode={rightPane.kind === 'settings' ? 'visible' : 'hidden'}>
            <Suspense fallback={<SettingsViewSkeleton />}>
              <QueryErrorBoundary>
                <SettingsView
                  onClose={() => setRightPane({ kind: 'profile' })}
                  onOpenMigration={async () => {
                    await migration.refresh()
                    setForceMigrationOpen(true)
                  }}
                  onOpenAbout={() => setDialog({ kind: 'about' })}
                />
              </QueryErrorBoundary>
            </Suspense>
          </Activity>
        </div>
      )}

      <CreateProfileDialog
        open={dialog.kind === 'create'}
        dependencies={dependencies.deps}
        submitting={submitting}
        onClose={() => setDialog({ kind: 'none' })}
        onCreate={handleCreate}
      />
      {managedSelected ? (
        <EditProfileDialog
          open={dialog.kind === 'edit'}
          profile={managedSelected}
          dependencies={dependencies.deps}
          submitting={submitting}
          onClose={() => setDialog({ kind: 'none' })}
          onSave={handleEdit}
        />
      ) : null}
      {managedSelected ? (
        <DeleteProfileDialog
          open={dialog.kind === 'delete'}
          profile={managedSelected}
          onClose={() => setDialog({ kind: 'none' })}
          onConfirm={handleDelete}
        />
      ) : null}

      <Suspense fallback={null}>
        <AboutDialog open={dialog.kind === 'about'} onClose={() => setDialog({ kind: 'none' })} />
      </Suspense>

      {showMigration ? (
        <MigrationDialog
          open
          existing={migration.existing}
          onClose={() => {
            setForceMigrationOpen(false)
          }}
          onImport={async (input) => {
            const imported = await migration.import(input)
            await profiles.refresh()
            selection.select(imported.id)
            setForceMigrationOpen(false)
            setRightPane({ kind: 'profile' })
            return imported
          }}
        />
      ) : null}

      {/* Mod+1..Mod+9 — one binding per managed profile slot (default row
          is not numbered). Disabled when any overlay is open to avoid
          stealing keystrokes from the dialog/palette/migration prompt. */}
      {entries
        .filter((entry): entry is Extract<SidebarEntry, { kind: 'managed' }> => entry.kind === 'managed')
        .slice(0, 9)
        .map((managedEntry, index) => (
          <SelectByIndexHotkey
            key={managedEntry.profile.id}
            index={index}
            enabled={!overlayOpen}
            onSelect={() => {
              selection.select(managedEntry.profile.id)
              setRightPane({ kind: 'profile' })
            }}
          />
        ))}

      <CommandPalette
        open={palette.open}
        entries={entries}
        selectedId={selection.selectedId}
        dependencies={dependencies.deps}
        onClose={palette.close}
        onSwitch={(id) => {
          selection.select(id)
          setRightPane({ kind: 'profile' })
        }}
        onLaunch={(profileId) => {
          void lastUsed.launchDesktop(profileId)
        }}
        onCopy={(profile) => {
          void lastUsed.copyCli({ profileId: profile.id, command: `claude-${profile.slug}` })
        }}
        onCreate={requestCreateProfile}
        onSettings={() => setRightPane({ kind: 'settings' })}
        onImport={async () => {
          await migration.refresh()
          setForceMigrationOpen(true)
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
