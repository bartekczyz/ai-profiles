import { Activity, Suspense, useEffect, useState } from 'react'

import { useTheme } from '@/design'
import { useDependencies } from '@/features/dependencies/api/use-dependencies'
import { useMigration } from '@/features/migration/api/use-migration'
import { MigrationDialog } from '@/features/migration/components/migration-dialog'
import { PathSetupBanner } from '@/features/onboarding/components/path-setup-banner'
import { WelcomeDialog } from '@/features/onboarding/components/welcome-dialog'
import { useProfiles } from '@/features/profiles/api/use-profiles'
import { CreateProfileModal } from '@/features/profiles/components/create-profile-modal'
import { DeleteProfileDialog } from '@/features/profiles/components/delete-profile-dialog'
import { EditProfileModal } from '@/features/profiles/components/edit-profile-modal'
import { EmptyStateScreen } from '@/features/profiles/components/empty-state-screen'
import { ProfileDetail } from '@/features/profiles/components/profile-detail'
import { ProfileDetailSkeleton } from '@/features/profiles/components/profile-detail-skeleton'
import { Sidebar } from '@/features/profiles/components/sidebar'
import { SidebarSkeleton } from '@/features/profiles/components/sidebar-skeleton'
import { SettingsView } from '@/features/settings/components/settings-view'
import { SettingsViewSkeleton } from '@/features/settings/components/settings-view-skeleton'
import { useAppState } from '@/lib/app-state/use-app-state'
import { QueryErrorBoundary } from '@/lib/query/error-boundary'

type ModalState = { kind: 'none' } | { kind: 'create' } | { kind: 'edit' } | { kind: 'delete' }

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

function AppContent() {
  const profiles = useProfiles()
  const migration = useMigration()
  const appState = useAppState()
  const dependencies = useDependencies()
  const [modal, setModal] = useState<ModalState>({ kind: 'none' })
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

  const selected = profiles.profiles.find((profile) => profile.id === profiles.selectedId) ?? null

  const shouldShowWelcome = !appState.state.welcomeShown

  const migrationDismissedRecently = isWithinDismissalWindow(appState.state.migrationDismissedAt)

  const shouldOfferMigration =
    appState.state.welcomeShown &&
    profiles.profiles.length === 0 &&
    migration.anyDetected &&
    !migrationDismissedRecently

  const showMigration = shouldOfferMigration || forceMigrationOpen

  const anyCliProfile = profiles.profiles.some((profile) => profile.surfaces.cli)
  const pathBannerDismissedRecently = isWithinDismissalWindow(appState.state.pathBannerDismissedAt)

  const shouldShowPathBanner =
    appState.state.welcomeShown &&
    dependencies.deps.localBinOnPath === false &&
    anyCliProfile &&
    !pathBannerDismissedRecently

  async function handleCreate(input: Parameters<typeof profiles.create>[0]) {
    setSubmitting(true)
    try {
      await profiles.create(input)
    } finally {
      setSubmitting(false)
    }
  }

  async function handleEdit(input: { name: string; color: string }) {
    if (!selected) {
      return
    }
    setSubmitting(true)
    try {
      await profiles.update({ id: selected.id, patch: input })
    } finally {
      setSubmitting(false)
    }
  }

  async function handleDelete(input: { moveToTrash: boolean }) {
    if (!selected) {
      return
    }
    await profiles.remove({ id: selected.id, ...input })
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

  // The empty-state screen owns the whole window when there are no profiles
  // yet — no sidebar, no panes. As soon as the first profile lands, the
  // sidebar appears and the detail pane takes over.
  const isEmpty = profiles.profiles.length === 0

  return (
    <div className="flex h-full flex-col">
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
        <EmptyStateScreen onCreate={() => setModal({ kind: 'create' })} />
      ) : (
        <div className="flex min-h-0 flex-1">
          <Sidebar
            profiles={profiles.profiles}
            selectedId={profiles.selectedId}
            onSelect={(id) => {
              profiles.select(id)
              setRightPane({ kind: 'profile' })
            }}
            onCreate={() => setModal({ kind: 'create' })}
            onSettings={() => setRightPane({ kind: 'settings' })}
          />
          {/* Activity keeps the off-screen pane mounted so toggling gear ↔ profile
              never re-fetches dependencies/backups or re-runs profile-detail effects. */}
          <Activity mode={rightPane.kind === 'profile' && selected !== null ? 'visible' : 'hidden'}>
            {selected ? (
              <Suspense fallback={<ProfileDetailSkeleton />}>
                <QueryErrorBoundary>
                  <ProfileDetail
                    profile={selected}
                    onEdit={() => setModal({ kind: 'edit' })}
                    onDelete={() => setModal({ kind: 'delete' })}
                    onToggle={async (surface, enabled) => {
                      await profiles.toggle({ id: selected.id, surface, enabled })
                    }}
                  />
                </QueryErrorBoundary>
              </Suspense>
            ) : null}
          </Activity>
          <Activity mode={rightPane.kind === 'settings' ? 'visible' : 'hidden'}>
            <Suspense fallback={<SettingsViewSkeleton />}>
              <QueryErrorBoundary>
                <SettingsView
                  onClose={() => setRightPane({ kind: 'profile' })}
                  onOpenMigration={async () => {
                    await migration.refresh()
                    setRightPane({ kind: 'profile' })
                    setForceMigrationOpen(true)
                  }}
                />
              </QueryErrorBoundary>
            </Suspense>
          </Activity>
        </div>
      )}

      <CreateProfileModal
        open={modal.kind === 'create'}
        dependencies={dependencies.deps}
        submitting={submitting}
        onClose={() => setModal({ kind: 'none' })}
        onCreate={handleCreate}
      />
      {selected ? (
        <EditProfileModal
          open={modal.kind === 'edit'}
          profile={selected}
          submitting={submitting}
          onClose={() => setModal({ kind: 'none' })}
          onSave={handleEdit}
        />
      ) : null}
      {selected ? (
        <DeleteProfileDialog
          open={modal.kind === 'delete'}
          profile={selected}
          onClose={() => setModal({ kind: 'none' })}
          onConfirm={handleDelete}
        />
      ) : null}

      {showMigration ? (
        <MigrationDialog
          open
          existing={migration.existing}
          onClose={async () => {
            setForceMigrationOpen(false)
            await appState.update({ migrationDismissedAt: new Date().toISOString() })
          }}
          onImport={async (input) => {
            const imported = await migration.import(input)
            await profiles.refresh()
            profiles.select(imported.id)
            setForceMigrationOpen(false)
            setRightPane({ kind: 'profile' })
            return imported
          }}
        />
      ) : null}
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
