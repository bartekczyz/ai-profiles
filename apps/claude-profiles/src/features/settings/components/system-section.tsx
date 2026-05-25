import type { StatusTone } from '@/design'
import type { Shell } from '@/lib/types'

import { useEffect, useState } from 'react'

import { RotateCw } from 'lucide-react'

import { Button, Skeleton, StatusDot } from '@/design'
import { useAppMetadata } from '@/features/about/api/use-app-metadata'
import { useDependencies } from '@/features/dependencies/api/use-dependencies'
import { type UpdaterStatus, useUpdater } from '@/features/updater/api/use-updater'
import { detectShell, installPathHook } from '@/lib/commands'

const rcDisplay: Record<Shell, string> = {
  zsh: '~/.zshrc',
  bash: '~/.bashrc',
  fish: '~/.config/fish/config.fish',
}

type Row = {
  label: string
  tone: StatusTone
  detail: string
  aux?: string
}

function describeUpdaterStatus(status: UpdaterStatus): { tone: StatusTone; detail: string } {
  switch (status.kind) {
    case 'idle':
      return { tone: 'neutral', detail: '—' }
    case 'checking':
      return { tone: 'neutral', detail: 'Checking…' }
    case 'up-to-date':
      return { tone: 'success', detail: 'Up to date' }
    case 'available':
      return { tone: 'warning', detail: `${status.update.version} available` }
    case 'installing':
      return { tone: 'warning', detail: 'Installing…' }
    case 'error':
      return { tone: 'warning', detail: status.message }
  }
}

// Version strings aren't surfaced from the Rust side yet (see 99-todo.md).
// Until then we render an em-dash next to each installed dependency — the
// status dot already conveys installed vs. not-detected.
const MISSING_DETAIL = '—'

const REFRESH_FLASH_MS = 1500

function buildRows(
  deps: { claudeAppInstalled: boolean; claudeCliInstalled: boolean; localBinOnPath: boolean },
  shell: Shell | null,
  updater: { tone: StatusTone; detail: string },
  version: string,
): Array<Row> {
  return [
    {
      label: 'Claude Desktop',
      tone: deps.claudeAppInstalled ? 'success' : 'warning',
      detail: MISSING_DETAIL,
    },
    {
      label: 'Claude Code CLI',
      tone: deps.claudeCliInstalled ? 'success' : 'warning',
      detail: MISSING_DETAIL,
    },
    {
      label: 'Shell PATH',
      tone: deps.localBinOnPath ? 'success' : 'warning',
      detail: shell ? rcDisplay[shell] : MISSING_DETAIL,
    },
    {
      label: 'Updates',
      tone: updater.tone,
      detail: updater.detail,
      aux: `v${version}`,
    },
  ]
}

/**
 * Consolidated System status card.
 *
 * Four rows (Desktop / CLI / Shell PATH / Updates) sit in one rounded card.
 * Each row shows a status dot (success/warning/neutral), a label, and a
 * mono detail string — version numbers if available (currently `—`
 * everywhere for deps, see follow-up in 99-todo.md), the resolved rc path
 * for the PATH row, or the live updater status. Beneath the card a
 * hookline lets the user re-install the shell hook in one click and an
 * updater hookline lets them trigger a manual update check.
 *
 * The section owns its own data: `useDependencies` suspends here (not at
 * the SettingsView level) so the rest of the Settings pane can paint
 * instantly while this card resolves.
 */
export function SystemSection() {
  const dependencies = useDependencies()
  const updater = useUpdater()
  const { version } = useAppMetadata()
  const [shell, setShell] = useState<Shell | null>(null)
  const [hookMessage, setHookMessage] = useState<string | null>(null)
  const [hookError, setHookError] = useState<string | null>(null)
  const [hookBusy, setHookBusy] = useState(false)
  const [refreshing, setRefreshing] = useState(false)
  const [refreshedAt, setRefreshedAt] = useState<number | null>(null)

  useEffect(() => {
    void detectShell().then(setShell)
  }, [])

  useEffect(() => {
    if (refreshedAt === null) {
      return
    }
    const handle = window.setTimeout(() => setRefreshedAt(null), REFRESH_FLASH_MS)
    return () => window.clearTimeout(handle)
  }, [refreshedAt])

  async function handleRefresh() {
    if (refreshing) {
      return
    }
    setRefreshing(true)
    setRefreshedAt(null)
    try {
      await Promise.all([dependencies.refresh(), detectShell().then(setShell)])
      setRefreshedAt(Date.now())
    } finally {
      setRefreshing(false)
    }
  }

  async function handleReinstall() {
    if (!shell) {
      return
    }
    setHookBusy(true)
    setHookMessage(null)
    setHookError(null)
    try {
      const outcome = await installPathHook(shell)
      if (outcome.outcome === 'alreadyInstalled') {
        setHookMessage(`${rcDisplay[shell]} already has the hook.`)
      } else {
        setHookMessage(`Updated ${rcDisplay[shell]}. Open a new terminal to pick it up.`)
      }
      await dependencies.refresh()
    } catch (caught) {
      setHookError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      setHookBusy(false)
    }
  }

  const updaterDescription = describeUpdaterStatus(updater.status)
  const rows = buildRows(dependencies.deps, shell, updaterDescription, version)
  const hookInstalled = dependencies.deps.localBinOnPath && shell !== null
  const updateCheckBusy = updater.status.kind === 'checking' || updater.status.kind === 'installing'
  const updateActionLabel = updater.status.kind === 'available' ? 'Restart and install' : 'Check now'
  const handleUpdateAction = updater.status.kind === 'available' ? updater.installAndRestart : updater.check

  return (
    <section className="mb-8">
      <div className="mb-2.5 flex items-center justify-between">
        <span className="font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-muted-strong">System</span>
        <div className="flex items-center gap-2">
          {refreshedAt !== null ? (
            <span className="font-mono text-[11px] text-muted-strong" role="status" aria-live="polite">
              Refreshed
            </span>
          ) : null}
          <Button
            size="sm"
            variant="secondary"
            leadingIcon={<RotateCw className={`h-3.5 w-3.5 ${refreshing ? 'animate-spin' : ''}`} strokeWidth={1.85} />}
            disabled={refreshing}
            onClick={() => void handleRefresh()}
          >
            {refreshing ? 'Refreshing…' : 'Refresh'}
          </Button>
        </div>
      </div>
      <div className="rounded-xl border border-border bg-white py-1 dark:bg-cream-2">
        {rows.map((row, index) => (
          <div
            // biome-ignore lint/suspicious/noArrayIndexKey: rows are a fixed tuple of three deps with no insert/reorder semantics
            key={index}
            className="grid grid-cols-[7px_1fr_auto] items-center gap-3 border-b border-border-soft px-4 py-3 text-[13px] tracking-[-0.003em] text-ink-soft last:border-b-0"
          >
            <StatusDot tone={row.tone} pulse />
            <span>
              {row.label}
              {row.aux ? <span className="ml-2 font-mono text-[11.5px] text-muted">{row.aux}</span> : null}
            </span>
            <span className="font-mono text-[11.5px] text-muted">{row.detail}</span>
          </div>
        ))}
      </div>
      <div className="mt-2.5 flex items-center gap-2.5 font-mono text-[11px] text-muted-strong">
        <span>
          {shell
            ? hookInstalled
              ? `Detected ${shell} — hook installed in ${rcDisplay[shell]}.`
              : `Detected ${shell} — hook not yet installed in ${rcDisplay[shell]}.`
            : 'Detecting your shell…'}
        </span>
        <Button size="sm" variant="ghost" disabled={!shell || hookBusy} onClick={handleReinstall}>
          {hookInstalled ? 'Re-install hook' : 'Install hook'}
        </Button>
      </div>
      {hookMessage ? (
        <p role="status" aria-live="polite" className="mt-1 text-[11.5px] text-muted-strong">
          {hookMessage}
        </p>
      ) : null}
      {hookError ? (
        <p role="alert" className="mt-1 text-[11.5px] text-red">
          {hookError}
        </p>
      ) : null}
      <div className="mt-2 flex items-center gap-2.5 font-mono text-[11px] text-muted-strong">
        <span>{updaterDescription.detail}</span>
        <Button size="sm" variant="ghost" disabled={updateCheckBusy} onClick={() => void handleUpdateAction()}>
          {updateActionLabel}
        </Button>
      </div>
    </section>
  )
}

/**
 * Skeleton placeholder for the System section while `useDependencies`
 * resolves. Kept colocated so the section's loading shape stays in sync
 * with its rendered shape.
 */
export function SystemSectionFallback() {
  return (
    <section className="mb-8">
      <div className="mb-2.5 flex items-center justify-between">
        <span className="font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-muted-strong">System</span>
        <Skeleton className="h-7 w-[88px] rounded-md" />
      </div>
      <Skeleton className="h-[126px] w-full rounded-xl" />
      <Skeleton className="mt-2.5 h-4 w-[280px] rounded-sm" />
    </section>
  )
}
