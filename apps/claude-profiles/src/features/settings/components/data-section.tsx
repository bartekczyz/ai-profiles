import type { AppId } from '@/lib/app-registry'
import type { ExistingInstallInfo } from '@/lib/types'

import { Download } from 'lucide-react'

import { ariaKeyshortcutsFor, Button, Kbd, Skeleton } from '@/design'
import { importableAppsFrom, useMigration } from '@/features/migration/api/use-migration'
import { useMigrationBackups } from '@/features/migration/api/use-migration-backups'
import { MigrationBackupsList } from '@/features/migration/components/migration-backups-list'
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
        <span className="font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-muted-strong">Data</span>
      </div>

      {importable.length > 0 ? (
        <div className="flex flex-col gap-2.5">
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
        <span className="font-mono text-[9.5px] font-medium uppercase tracking-[0.1em] text-muted-strong">
          Backups · {backups.backups.length}
        </span>
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
 * One per-app "Detected an existing {App} install." action card. The ⌘I
 * shortcut chip rides only the first card, since ⌘I opens the first
 * importable app.
 */
function DetectedInstallCard({ app, detected, showShortcut, onReimport }: DetectedInstallCardProps) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-xl border border-border bg-white px-4 py-3.5 dark:bg-cream-2">
      <div className="flex min-w-0 items-center gap-3">
        <span
          aria-hidden
          className="grid h-7 w-7 shrink-0 place-items-center rounded-[7px] bg-cream-2 text-ink-soft dark:bg-white/[0.04]"
        >
          <Download className="h-4 w-4" strokeWidth={1.85} />
        </span>
        <div className="min-w-0">
          <div className="text-[13px] tracking-[-0.005em] text-ink">
            Detected an existing {appSpecs[app].displayName} install.
          </div>
          <div className="mt-0.5 truncate font-mono text-[11px] text-muted-strong">
            {detected.size !== null ? `${formatBytes(detected.size)} · ` : ''}
            {detected.path}/
          </div>
        </div>
      </div>
      <Button
        size="sm"
        variant="secondary"
        trailingKbd={showShortcut ? <Kbd shortcutId="open-detect-import" /> : undefined}
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
        <span className="font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-muted-strong">Data</span>
      </div>
      <Skeleton className="h-[60px] w-full rounded-xl" />
      <div className="mt-[18px] mb-2 flex items-center justify-between gap-3">
        <Skeleton className="h-3 w-24 rounded-sm" />
        <Skeleton className="h-3 w-40 rounded-sm" />
      </div>
      <Skeleton className="h-[52px] w-full rounded-xl" />
    </section>
  )
}
