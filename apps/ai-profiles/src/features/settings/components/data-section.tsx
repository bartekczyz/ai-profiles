import type { AppId } from '@/lib/app-registry'
import type { ExistingInstallInfo } from '@/lib/types'

import { DownloadSimple } from '@phosphor-icons/react'

import { ariaKeyshortcutsFor, Button, cn, panelClasses, Skeleton } from '@/design'
import { importableAppsFrom, useMigration } from '@/features/migration/api/use-migration'
import { useMigrationBackups } from '@/features/migration/api/use-migration-backups'
import { MigrationBackupsList } from '@/features/migration/components/migration-backups-list'
import { shortenHomePath } from '@/features/profiles/components/shorten-home-path'
import { appSpecs } from '@/lib/app-registry'
import { formatBytes } from '@/lib/format-bytes'

type Props = {
  onReimport: (app: AppId) => void
}

/**
 * Picks the "primary" detected install for the Data action card.
 * The card shows a single line, so we surface the GUI path when present,
 * otherwise the CLI path. The sub-string formats `{size} · {path}/` to
 * match the prototype.
 */
function actionDetail(existing: ExistingInstallInfo): { path: string; size: number | null } | null {
  if (existing.guiPath !== null) {
    return { path: existing.guiPath, size: existing.guiSizeBytes }
  }
  if (existing.cliPath !== null) {
    return { path: existing.cliPath, size: existing.cliSizeBytes }
  }
  return null
}

/**
 * The Data section owns its own loading: both `useMigration` (filesystem
 * walk to detect existing installs) and `useMigrationBackups` (lists
 * `migration-backup-*` dirs) suspend here so the Appearance + System
 * sections above don't wait on either.
 */
export function DataSection({ onReimport }: Props) {
  const claudeMigration = useMigration('claude')
  const codexMigration = useMigration('codex')
  const backups = useMigrationBackups()
  const existingByApp: Record<AppId, ExistingInstallInfo> = {
    claude: claudeMigration.existing,
    codex: codexMigration.existing,
  }
  const importable = importableAppsFrom(existingByApp)

  return (
    <section className="mb-8">
      <div className="mb-2.5 flex items-center justify-between">
        <span className="text-[12.5px] font-medium text-ink-soft">Data</span>
      </div>

      {importable.length > 0 ? (
        <div className={cn(panelClasses, 'divide-y divide-border-soft overflow-hidden')}>
          {importable.map((app, index) => {
            const detected = actionDetail(existingByApp[app])
            if (detected === null) {
              return null
            }
            return (
              <DetectedInstallCard
                key={app}
                app={app}
                detected={detected}
                showShortcut={index === 0}
                onReimport={() => onReimport(app)}
              />
            )
          })}
        </div>
      ) : null}

      <div className={`flex items-center justify-between gap-3 ${importable.length > 0 ? 'mt-[18px]' : ''} mb-2`}>
        <span className="text-[12.5px] font-medium text-ink-soft">Backups · {backups.backups.length}</span>
        <span className="text-[11px] tracking-[-0.003em] text-muted-strong">Removed automatically after 7 days.</span>
      </div>
      <MigrationBackupsList backups={backups.backups} onDelete={backups.remove} />
    </section>
  )
}

type DetectedInstallCardProps = {
  app: AppId
  detected: { path: string; size: number | null }
  showShortcut: boolean
  onReimport: () => void
}

/**
 * One per-app row of the detected-installs list: what was found, where, and
 * its Re-import. The ⌘I shortcut rides only the first row, since ⌘I opens the
 * first importable app.
 */
function DetectedInstallCard({ app, detected, showShortcut, onReimport }: DetectedInstallCardProps) {
  return (
    <div className="flex items-center justify-between gap-4 px-4 py-3">
      <div className="flex min-w-0 items-center gap-3">
        <span
          aria-hidden
          className="grid h-7 w-7 shrink-0 place-items-center rounded-md border border-border bg-cream-2 text-muted dark:bg-white/[0.04]"
        >
          <DownloadSimple className="h-4 w-4" />
        </span>
        <div className="min-w-0">
          <div className="text-[13px] tracking-[-0.005em] text-ink">
            Existing {appSpecs[app].displayName} install
            {detected.size !== null ? (
              <span className="ml-2 text-[12px] text-muted-strong">{formatBytes(detected.size)}</span>
            ) : null}
          </div>
          <div className="mt-0.5 truncate font-mono text-[11.5px] text-muted-strong" title={detected.path}>
            {shortenHomePath(detected.path)}
          </div>
        </div>
      </div>
      <Button
        size="sm"
        variant="secondary"
        shortcut={showShortcut ? 'open-detect-import' : undefined}
        aria-keyshortcuts={showShortcut ? ariaKeyshortcutsFor('open-detect-import') : undefined}
        onClick={onReimport}
      >
        Re-import…
      </Button>
    </div>
  )
}

/**
 * Skeleton placeholder for the Data section while its suspending queries
 * resolve. Approximates the action-card + backups-list shape so the
 * pane doesn't jump when the data lands.
 */
export function DataSectionFallback() {
  return (
    <section className="mb-8">
      <div className="mb-2.5 flex items-center justify-between">
        <span className="text-[12.5px] font-medium text-ink-soft">Data</span>
      </div>
      <Skeleton className="h-[60px] w-full rounded-[10px]" />
      <div className="mt-[18px] mb-2 flex items-center justify-between gap-3">
        <Skeleton className="h-3 w-24 rounded-sm" />
        <Skeleton className="h-3 w-40 rounded-sm" />
      </div>
      <Skeleton className="h-[52px] w-full rounded-[10px]" />
    </section>
  )
}
