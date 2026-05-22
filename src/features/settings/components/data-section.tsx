import type { ExistingInstallInfo } from '@/lib/types'

import { Download } from 'lucide-react'

import { ariaKeyshortcutsFor, Button, Kbd, Skeleton } from '@/design'
import { useMigration } from '@/features/migration/api/use-migration'
import { useMigrationBackups } from '@/features/migration/api/use-migration-backups'
import { MigrationBackupsList } from '@/features/migration/components/migration-backups-list'
import { formatBytes } from '@/lib/format-bytes'

type Props = {
  onReimport: () => void | Promise<void>
}

/**
 * Picks the "primary" detected install for the Data action card.
 * The card shows a single line, so we surface Claude Desktop when present,
 * otherwise Claude Code CLI. The sub-string formats `{size} · {path}/` to
 * match the prototype.
 */
function actionDetail(existing: ExistingInstallInfo): { path: string; size: number | null } | null {
  if (existing.claudeDesktopPath !== null) {
    return { path: existing.claudeDesktopPath, size: existing.claudeDesktopSizeBytes }
  }
  if (existing.claudeCodePath !== null) {
    return { path: existing.claudeCodePath, size: existing.claudeCodeSizeBytes }
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
  const migration = useMigration()
  const backups = useMigrationBackups()
  const detected = actionDetail(migration.existing)

  return (
    <section className="mb-8">
      <div className="mb-2.5 flex items-center justify-between">
        <span className="font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-muted-strong">Data</span>
      </div>

      {detected ? (
        <div className="flex items-center justify-between gap-4 rounded-xl border border-border bg-white px-4 py-3.5 dark:bg-cream-2">
          <div className="flex min-w-0 items-center gap-3">
            <span
              aria-hidden
              className="grid h-7 w-7 shrink-0 place-items-center rounded-[7px] bg-cream-2 text-ink-soft dark:bg-white/[0.04]"
            >
              <Download className="h-4 w-4" strokeWidth={1.85} />
            </span>
            <div className="min-w-0">
              <div className="text-[13px] tracking-[-0.005em] text-ink">Detected an existing Claude install.</div>
              <div className="mt-0.5 truncate font-mono text-[11px] text-muted-strong">
                {detected.size !== null ? `${formatBytes(detected.size)} · ` : ''}
                {detected.path}/
              </div>
            </div>
          </div>
          <Button
            size="sm"
            variant="secondary"
            trailingKbd={<Kbd shortcutId="open-detect-import" />}
            aria-keyshortcuts={ariaKeyshortcutsFor('open-detect-import')}
            onClick={() => void onReimport()}
          >
            Re-import…
          </Button>
        </div>
      ) : null}

      <div className={`flex items-center justify-between gap-3 ${detected ? 'mt-[18px]' : ''} mb-2`}>
        <span className="font-mono text-[9.5px] font-medium uppercase tracking-[0.1em] text-muted-strong">
          Backups · {backups.backups.length}
        </span>
        <span className="text-[11px] tracking-[-0.003em] text-muted-strong">Removed automatically after 7 days.</span>
      </div>
      <MigrationBackupsList backups={backups.backups} onDelete={backups.remove} />
    </section>
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
