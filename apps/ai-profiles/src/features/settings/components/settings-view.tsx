import type { AppId } from '@/lib/app-registry'

import { Suspense } from 'react'

import { PaneLayout } from '@/components/pane-layout'
import { Button, Kbd } from '@/design'

import { AppearanceSection } from './appearance-section'
import { DataSection, DataSectionFallback } from './data-section'
import { RemoteHostsSection, RemoteHostsSectionFallback } from './remote-hosts-section'
import { SettingsFooterRow } from './settings-footer-row'
import { SystemSection, SystemSectionFallback } from './system-section'

type Props = {
  onClose: () => void
  onOpenMigration: (app: AppId) => void
  onOpenAbout: () => void
}

/**
 * Settings shell.
 *
 * The header (pinned by `PaneLayout`), Appearance section, and footer row
 * all render synchronously.
 * The two suspending sections (System owns `useDependencies`, Data owns
 * `useMigration` + `useMigrationBackups`) sit behind their own Suspense
 * boundaries so the rest of the pane paints instantly when the user opens
 * Settings — previously a single outer boundary made the whole pane wait
 * on the filesystem walk in `detect_existing_install`.
 */
export function SettingsView({ onClose, onOpenMigration, onOpenAbout }: Props) {
  return (
    <PaneLayout
      className="bg-background"
      header={
        <header className="flex items-center justify-between border-b border-border-soft pb-6">
          <h2 className="text-[22px] font-semibold tracking-[-0.012em] text-ink">Settings</h2>
          <Button size="sm" variant="ghost" trailingKbd={<Kbd>⎋</Kbd>} aria-keyshortcuts="Escape" onClick={onClose}>
            Done
          </Button>
        </header>
      }
    >
      <AppearanceSection />
      <Suspense fallback={<SystemSectionFallback />}>
        <SystemSection />
      </Suspense>
      <Suspense fallback={<RemoteHostsSectionFallback />}>
        <RemoteHostsSection />
      </Suspense>
      <Suspense fallback={<DataSectionFallback />}>
        <DataSection onReimport={onOpenMigration} />
      </Suspense>
      <SettingsFooterRow onOpenAbout={onOpenAbout} />
    </PaneLayout>
  )
}
