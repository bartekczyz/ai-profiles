import { useState } from 'react'

import { CreateProfileModal } from '@/components/create-profile-modal'
import { DeleteProfileDialog } from '@/components/delete-profile-dialog'
import { EditProfileModal } from '@/components/edit-profile-modal'
import { EmptyState } from '@/components/empty-state'
import { MigrationDialog } from '@/components/migration-dialog'
import { PathSetupBanner } from '@/components/path-setup-banner'
import { ProfileDetail } from '@/components/profile-detail'
import { Sidebar } from '@/components/sidebar'
import { WelcomeDialog } from '@/components/welcome-dialog'
import { useAppState } from '@/hooks/use-app-state'
import { useDependencies } from '@/hooks/use-dependencies'
import { useMigration } from '@/hooks/use-migration'
import { useProfiles } from '@/hooks/use-profiles'

type ModalState = { kind: 'none' } | { kind: 'create' } | { kind: 'edit' } | { kind: 'delete' }

const SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000
const DISMISSAL_WINDOW_MS = 7 * SEVEN_DAYS_MS

function isWithinDismissalWindow(timestamp: string | null | undefined): boolean {
  if (!timestamp) {
    return false
  }
  return Date.now() - new Date(timestamp).getTime() < DISMISSAL_WINDOW_MS
}

export default function App() {
  const profiles = useProfiles()
  const migration = useMigration()
  const appState = useAppState()
  const dependencies = useDependencies()
  const [modal, setModal] = useState<ModalState>({ kind: 'none' })
  const [submitting, setSubmitting] = useState(false)

  const selected = profiles.profiles.find((profile) => profile.id === profiles.selectedId) ?? null

  const shouldShowWelcome = !appState.loading && appState.state !== null && !appState.state.welcomeShown

  const migrationDismissedRecently = isWithinDismissalWindow(appState.state?.migrationDismissedAt)

  const shouldOfferMigration =
    !profiles.loading &&
    !migration.loading &&
    !appState.loading &&
    appState.state?.welcomeShown === true &&
    profiles.profiles.length === 0 &&
    migration.anyDetected &&
    !migrationDismissedRecently

  const anyCliProfile = profiles.profiles.some((profile) => profile.surfaces.cli)
  const pathBannerDismissedRecently = isWithinDismissalWindow(appState.state?.pathBannerDismissedAt)

  const shouldShowPathBanner =
    !profiles.loading &&
    !dependencies.loading &&
    !appState.loading &&
    appState.state?.welcomeShown === true &&
    dependencies.deps?.localBinOnPath === false &&
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

  if (profiles.loading) {
    return <div className="flex h-full items-center justify-center text-sm">Loading…</div>
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
      <div className="flex min-h-0 flex-1">
        <Sidebar
          profiles={profiles.profiles}
          selectedId={profiles.selectedId}
          onSelect={profiles.select}
          onCreate={() => setModal({ kind: 'create' })}
        />
        {selected ? (
          <ProfileDetail
            profile={selected}
            onEdit={() => setModal({ kind: 'edit' })}
            onDelete={() => setModal({ kind: 'delete' })}
            onToggle={async (surface, enabled) => {
              await profiles.toggle({ id: selected.id, surface, enabled })
            }}
          />
        ) : (
          <EmptyState onCreate={() => setModal({ kind: 'create' })} />
        )}
      </div>

      {dependencies.deps ? (
        <CreateProfileModal
          open={modal.kind === 'create'}
          dependencies={dependencies.deps}
          submitting={submitting}
          onClose={() => setModal({ kind: 'none' })}
          onCreate={handleCreate}
        />
      ) : null}
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

      {shouldOfferMigration && migration.existing ? (
        <MigrationDialog
          open
          existing={migration.existing}
          onClose={async () => {
            await appState.update({ migrationDismissedAt: new Date().toISOString() })
          }}
          onImport={async (input) => {
            const imported = await migration.import(input)
            await profiles.refresh()
            profiles.select(imported.id)
            return imported
          }}
        />
      ) : null}

      {profiles.error ? (
        <div className="pointer-events-none absolute right-4 bottom-4 max-w-sm rounded-md bg-red-600 px-3 py-2 text-sm text-white shadow">
          {profiles.error}
        </div>
      ) : null}
    </div>
  )
}
