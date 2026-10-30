import type { ReactNode } from 'react'
import type { StatusTone } from '@/design'
import type { Dependencies, Shell } from '@/lib/types'

import { useEffect, useState } from 'react'

import { ArrowClockwise } from '@phosphor-icons/react'

import { Button, Skeleton, StatusDot } from '@/design'
import { useAppMetadata } from '@/features/about/api/use-app-metadata'
import { useDependencies } from '@/features/dependencies/api/use-dependencies'
import { hookInstallMessage, rcDisplay, shellHookStatus, updateAction } from '@/features/settings/lib/system-status'
import { type UpdaterStatus, useUpdater } from '@/features/updater/api/use-updater'
import { appIds, appSpecs } from '@/lib/app-registry'
import { detectShell, installPathHook } from '@/lib/commands'

type Row = {
  /**
   * Which row this is, for the rows that carry a control of their own.
   */
  id: 'app' | 'path' | 'updates'
  label: string
  tone: StatusTone
  detail: string
  /**
   * Whether the detail is a path, set in mono; everything else reads as text.
   */
  mono?: boolean
  aux?: string
  /**
   * A longer account of the row's state, shown as its tooltip.
   */
  hint?: string
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

// Version strings aren't surfaced from the Rust side yet (see 99-todo.md), so
// an app row says whether it was found, beside the dot that says the same.
const MISSING_DETAIL = '—'

function presence(found: boolean): string {
  return found ? 'Installed' : 'Not found'
}

const REFRESH_FLASH_MS = 1500

function buildAppRows(dependencies: Dependencies): Array<Row> {
  return appIds.flatMap((id) => {
    const spec = appSpecs[id]
    const deps = dependencies.apps[id]
    return [
      {
        id: 'app',
        label: `${spec.displayName} Desktop`,
        tone: deps.guiInstalled ? 'success' : 'warning',
        detail: presence(deps.guiInstalled),
      },
      {
        id: 'app',
        label: `${spec.cliDisplayName} CLI`,
        tone: deps.cliInstalled ? 'success' : 'warning',
        detail: presence(deps.cliInstalled),
      },
    ]
  })
}

function buildRows(
  deps: Dependencies,
  shell: Shell | null,
  updater: { tone: StatusTone; detail: string },
  version: string,
): Array<Row> {
  return [
    ...buildAppRows(deps),
    {
      id: 'path',
      label: 'Shell PATH',
      tone: deps.localBinOnPath ? 'success' : 'warning',
      detail: shell ? rcDisplay[shell] : MISSING_DETAIL,
      mono: shell !== null,
      hint: shellHookStatus(shell, deps.localBinOnPath && shell !== null),
    },
    {
      id: 'updates',
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
 * Rows: two per managed app (Desktop / CLI) from the registry, plus Shell
 * PATH and Updates — six rows in total when both Claude and ChatGPT are
 * registered. Each row shows a status dot (success/warning/neutral), a
 * label, and a mono detail string. Beneath the card a hookline lets the
 * user re-install the shell hook in one click and an updater hookline lets
 * them trigger a manual update check.
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

  useEffect(() => {
    void detectShell().then(setShell)
  }, [])

  async function refresh() {
    await Promise.all([dependencies.refresh(), detectShell().then(setShell)])
  }

  const updaterDescription = describeUpdaterStatus(updater.status)
  const rows = buildRows(dependencies.deps, shell, updaterDescription, version)
  const action = updateAction(updater.status)
  const handleUpdateAction = action.installs ? updater.installAndRestart : updater.check

  const hook = useShellHook({
    localBinOnPath: dependencies.deps.localBinOnPath,
    shell,
    onInstalled: dependencies.refresh,
  })

  // The two rows that can be acted on carry their control at the row's end,
  // where a Mac settings pane puts it, rather than in a line under the card.
  const controls: Partial<Record<Row['id'], ReactNode>> = {
    path: (
      <Button size="sm" variant="secondary" disabled={!shell || hook.busy} onClick={() => void hook.install()}>
        {hook.installed ? 'Re-install hook' : 'Install hook'}
      </Button>
    ),
    updates: (
      <Button size="sm" variant="secondary" disabled={action.busy} onClick={() => void handleUpdateAction()}>
        {action.label}
      </Button>
    ),
  }

  return (
    <section className="mb-8">
      <div className="mb-2.5 flex items-center justify-between">
        <span className="text-[12.5px] font-medium text-ink-soft">System</span>
        <RefreshControl onRefresh={refresh} />
      </div>
      <div className="overflow-hidden rounded-[10px] border border-border bg-cream/70 dark:bg-white/[0.02]">
        {rows.map((row, index) => (
          <StatusRow
            // biome-ignore lint/suspicious/noArrayIndexKey: rows are a stable ordered list with no insert/reorder semantics
            key={index}
            row={row}
            control={controls[row.id]}
          />
        ))}
      </div>
      {hook.message ? (
        <p role="status" aria-live="polite" className="mt-2 px-1 text-[12px] text-muted">
          {hook.message}
        </p>
      ) : null}
      {hook.error ? (
        <p role="alert" className="mt-2 px-1 text-[12px] text-red">
          {hook.error}
        </p>
      ) : null}
    </section>
  )
}

type RefreshControlProps = {
  /**
   * Re-reads the dependencies and the shell; resolves once both are back.
   */
  onRefresh: () => Promise<unknown>
}

/**
 * The header's Refresh button, spinning while a refresh runs and followed by a
 * brief "Refreshed" confirmation once it lands.
 */
function RefreshControl({ onRefresh }: RefreshControlProps) {
  const [refreshing, setRefreshing] = useState(false)
  const [refreshedAt, setRefreshedAt] = useState<number | null>(null)

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
      await onRefresh()
      setRefreshedAt(Date.now())
    } finally {
      setRefreshing(false)
    }
  }

  return (
    <div className="flex items-center gap-2">
      {refreshedAt !== null ? (
        <span className="text-[12px] text-muted-strong" role="status" aria-live="polite">
          Refreshed
        </span>
      ) : null}
      <Button
        size="sm"
        variant="secondary"
        leadingIcon={<ArrowClockwise className={`h-3.5 w-3.5 ${refreshing ? 'animate-spin' : ''}`} />}
        disabled={refreshing}
        onClick={() => void handleRefresh()}
      >
        {refreshing ? 'Refreshing…' : 'Refresh'}
      </Button>
    </div>
  )
}

type StatusRowProps = {
  /**
   * The dependency or subsystem the row reports on.
   */
  row: Row
  /**
   * The row's own control, after its detail.
   */
  control?: ReactNode
}

/**
 * One line of the System card: a status dot, a label, the detail, and the
 * row's control when it has one.
 */
function StatusRow({ row, control }: StatusRowProps) {
  return (
    <div
      title={row.hint}
      className="flex min-h-12 items-center gap-3 border-b border-border-soft px-4 py-2 text-[13px] tracking-[-0.003em] text-ink last:border-b-0"
    >
      <StatusDot tone={row.tone} />
      <span className="min-w-0 flex-1 truncate">
        {row.label}
        {row.aux ? <span className="ml-2 text-[12px] text-muted-strong">{row.aux}</span> : null}
      </span>
      <span className={row.mono ? 'font-mono text-[12px] text-muted' : 'text-[12.5px] text-muted'}>{row.detail}</span>
      {control}
    </div>
  )
}

type ShellHookOptions = {
  /**
   * Whether `~/.local/bin` is on the user's PATH.
   */
  localBinOnPath: boolean
  /**
   * The detected shell, or `null` while detection runs.
   */
  shell: Shell | null
  /**
   * Re-reads the dependencies once the hook has been written.
   */
  onInstalled: () => Promise<unknown>
}

/**
 * The PATH hook's one-click (re-)install, and what came of the last attempt.
 */
function useShellHook({ localBinOnPath, shell, onInstalled }: ShellHookOptions) {
  const [message, setMessage] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function install() {
    if (!shell) {
      return
    }
    setBusy(true)
    setMessage(null)
    setError(null)
    try {
      const outcome = await installPathHook(shell)
      setMessage(hookInstallMessage(shell, outcome))
      await onInstalled()
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      setBusy(false)
    }
  }

  return { installed: localBinOnPath && shell !== null, busy, message, error, install }
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
        <span className="text-[12.5px] font-medium text-ink-soft">System</span>
        <Skeleton className="h-7 w-[88px] rounded-md" />
      </div>
      <Skeleton className="h-[190px] w-full rounded-xl" />
      <Skeleton className="mt-2.5 h-4 w-[280px] rounded-sm" />
    </section>
  )
}
