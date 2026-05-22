import { Suspense } from 'react'

import { Button, Kbd } from '@/design'

import { AppearanceSection } from './appearance-section'
import { DataSection, DataSectionFallback } from './data-section'
import { SettingsFooterRow } from './settings-footer-row'
import { SystemSection, SystemSectionFallback } from './system-section'

type Props = {
  onClose: () => void
  onOpenMigration: () => void
  onOpenAbout: () => void
}

/**
 * Settings shell.
 *
 * The header, Appearance section, and footer row all render synchronously.
 * The two suspending sections (System owns `useDependencies`, Data owns
 * `useMigration` + `useMigrationBackups`) sit behind their own Suspense
 * boundaries so the rest of the pane paints instantly when the user opens
 * Settings — previously a single outer boundary made the whole pane wait
 * on the filesystem walk in `detect_existing_claude_install`.
 */
export function SettingsView({ onClose, onOpenMigration, onOpenAbout }: Props) {
  return (
    <main className="flex flex-1 flex-col overflow-y-auto bg-background px-9 py-9">
      <div className="mx-auto flex w-full max-w-[640px] flex-1 flex-col">
        <header className="mb-6 flex items-center justify-between">
          <h2 className="text-[22px] font-semibold tracking-[-0.012em] text-ink">Settings</h2>
          <Button size="sm" variant="ghost" trailingKbd={<Kbd>⎋</Kbd>} aria-keyshortcuts="Escape" onClick={onClose}>
            Done
          </Button>
        </header>
        <div className="mb-6 h-px bg-border-soft" />

        <AppearanceSection />
        <Suspense fallback={<SystemSectionFallback />}>
          <SystemSection />
        </Suspense>
        <Suspense fallback={<DataSectionFallback />}>
          <DataSection onReimport={onOpenMigration} />
        </Suspense>
        <SettingsFooterRow onOpenAbout={onOpenAbout} />
      </div>
    </main>
  )
}
