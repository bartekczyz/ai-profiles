import type { MigrationLauncher } from '@/features/migration/api/use-migration-launcher'
import type { Profile } from '@/lib/types'

import { MigrationDialog } from './migration-dialog'

type Props = {
  /**
   * Which app's import dialog is open, and its import.
   */
  launcher: MigrationLauncher
  /**
   * Called with the imported profile, before the dialog closes.
   */
  onImported: (profile: Profile) => Promise<void>
}

/**
 * Mounts the import dialog for the launcher's open app, and nothing while
 * it is closed.
 */
export function MigrationDialogHost({ launcher, onImported }: Props) {
  const { app, active } = launcher
  if (app === null || active === null) {
    return null
  }

  return (
    <MigrationDialog
      open
      app={app}
      existing={active.existing}
      onClose={launcher.close}
      onImport={async (input) => {
        const imported = await active.import(input)
        await onImported(imported)
        launcher.close()
        return imported
      }}
    />
  )
}
