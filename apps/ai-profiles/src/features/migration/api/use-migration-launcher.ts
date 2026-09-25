import type { AppId } from '@/lib/app-registry'

import { useState } from 'react'

import { importableAppsFrom, useMigration } from './use-migration'

/**
 * The import dialog's open state and every way into it.
 */
export type MigrationLauncher = {
  /**
   * The app whose import dialog is open, or `null` when closed.
   */
  app: AppId | null
  /**
   * The open app's detection and import, or `null` when closed.
   */
  active: ReturnType<typeof useMigration> | null
  /**
   * Apps with a detected stock install, in `appIds` order.
   */
  importableApps: Array<AppId>
  /**
   * Refreshes `app`'s detection, then opens its import dialog.
   */
  open: (app: AppId) => Promise<void>
  /**
   * Opens the first importable app's dialog, if any (⌘I).
   */
  openFirstImportable: () => void
  /**
   * Closes the import dialog.
   */
  close: () => void
}

/**
 * Owns which app's import dialog is open. The originating surface
 * (default-entry "Migrate", Settings, palette, ⌘I) picks the app so a
 * ChatGPT default opens a ChatGPT import, not the Claude one.
 */
export function useMigrationLauncher(): MigrationLauncher {
  const claudeMigration = useMigration('claude')
  const codexMigration = useMigration('codex')
  const [app, setApp] = useState<AppId | null>(null)

  const migrationByApp: Record<AppId, ReturnType<typeof useMigration>> = {
    claude: claudeMigration,
    codex: codexMigration,
  }

  // Apps with a detected stock install — drives every "Detect and import"
  // entry point (⌘I, palette, Settings) so ChatGPT is reachable, not just Claude.
  const importableApps = importableAppsFrom({
    claude: claudeMigration.existing,
    codex: codexMigration.existing,
  })

  async function open(target: AppId) {
    await migrationByApp[target].refresh()
    setApp(target)
  }

  return {
    app,
    active: app ? migrationByApp[app] : null,
    importableApps,
    open,
    openFirstImportable: () => {
      const target = importableApps[0]
      if (target) {
        void open(target)
      }
    },
    close: () => setApp(null),
  }
}
