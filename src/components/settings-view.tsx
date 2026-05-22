import type { PathHookOutcome, Shell } from '@/lib/types'

import { useEffect, useState } from 'react'

import { getVersion } from '@tauri-apps/api/app'

import { Button } from '@/components/ui/button'
import { useAppState } from '@/hooks/use-app-state'
import { useDependencies } from '@/hooks/use-dependencies'
import { useMigrationBackups } from '@/hooks/use-migration-backups'
import { detectExistingClaudeInstall, detectShell, installPathHook } from '@/lib/commands'

import { MigrationBackupsList } from './migration-backups-list'

type Props = {
  onClose: () => void
  onOpenMigration: () => void
}

const RC_DISPLAY: Record<Shell, string> = {
  zsh: '~/.zshrc',
  bash: '~/.bashrc',
  fish: '~/.config/fish/config.fish',
}

export function SettingsView({ onClose, onOpenMigration }: Props) {
  const dependencies = useDependencies()
  const backups = useMigrationBackups()
  const appState = useAppState()

  const [version, setVersion] = useState<string>('')
  const [shell, setShell] = useState<Shell | null>(null)
  const [reimportMessage, setReimportMessage] = useState<string | null>(null)
  const [pathMessage, setPathMessage] = useState<string | null>(null)
  const [pathError, setPathError] = useState<string | null>(null)
  const [resetMessage, setResetMessage] = useState<string | null>(null)

  useEffect(() => {
    void getVersion().then(setVersion)
    void detectShell().then(setShell)
  }, [])

  async function handleManualReimport() {
    setReimportMessage(null)
    const existing = await detectExistingClaudeInstall()
    if (existing.claudeDesktopPath === null && existing.claudeCodePath === null) {
      setReimportMessage('No existing Claude installs detected on this Mac.')
      return
    }
    await appState.update({ clearMigrationDismissed: true })
    onOpenMigration()
  }

  async function handleInstallPath() {
    if (!shell) {
      return
    }
    setPathMessage(null)
    setPathError(null)
    try {
      const outcome: PathHookOutcome = await installPathHook(shell)
      if (outcome.outcome === 'alreadyInstalled') {
        setPathMessage(`${RC_DISPLAY[shell]} already has the hook.`)
      } else {
        setPathMessage(`Updated ${RC_DISPLAY[shell]}. Open a new terminal to pick it up.`)
      }
      await dependencies.refresh()
    } catch (caught) {
      setPathError(caught instanceof Error ? caught.message : String(caught))
    }
  }

  async function handleResetOnboarding() {
    setResetMessage(null)
    await appState.update({
      welcomeShown: false,
      clearMigrationDismissed: true,
      clearPathBannerDismissed: true,
    })
    setResetMessage('Onboarding flags reset. Restart the app to see the welcome flow.')
  }

  return (
    <main className="flex flex-1 flex-col overflow-y-auto bg-background p-6">
      <header className="mb-6 flex items-center justify-between">
        <h2 className="text-xl font-semibold">Settings</h2>
        <Button size="sm" variant="ghost" onClick={onClose}>
          Done
        </Button>
      </header>

      <section className="mb-6">
        <h3 className="mb-2 text-sm font-semibold">App</h3>
        <p className="text-sm text-muted-foreground">
          claude-profiles {version || '…'} — run multiple Claude accounts on one Mac.
        </p>
        <p className="mt-2 text-xs text-muted-foreground">Not affiliated with Anthropic.</p>
      </section>

      <section className="mb-6">
        <div className="mb-2 flex items-center justify-between">
          <h3 className="text-sm font-semibold">Dependencies</h3>
          <Button size="sm" variant="ghost" onClick={() => dependencies.refresh()}>
            Refresh
          </Button>
        </div>
        {dependencies.deps ? (
          <ul className="space-y-1 text-sm">
            <li>Claude Desktop: {dependencies.deps.claudeAppInstalled ? '✓ installed' : '✗ not found'}</li>
            <li>Claude Code CLI: {dependencies.deps.claudeCliInstalled ? '✓ installed' : '✗ not found'}</li>
            <li>
              <code>~/.local/bin</code> on PATH: {dependencies.deps.localBinOnPath ? '✓ yes' : '✗ no'}
            </li>
          </ul>
        ) : (
          <p className="text-sm text-muted-foreground">Loading…</p>
        )}
      </section>

      <section className="mb-6">
        <h3 className="mb-2 text-sm font-semibold">Migration backups</h3>
        {backups.loading ? (
          <p className="text-sm text-muted-foreground">Loading…</p>
        ) : (
          <MigrationBackupsList backups={backups.backups} onDelete={backups.remove} />
        )}
      </section>

      <section className="mb-6">
        <h3 className="mb-2 text-sm font-semibold">Import an existing install</h3>
        <p className="mb-2 text-xs text-muted-foreground">
          If you skipped the first-run import or installed Claude after setting up claude-profiles, you can re-run the
          detection here.
        </p>
        <Button size="sm" onClick={handleManualReimport}>
          Detect and import…
        </Button>
        {reimportMessage ? <p className="mt-2 text-sm text-muted-foreground">{reimportMessage}</p> : null}
      </section>

      <section className="mb-6">
        <h3 className="mb-2 text-sm font-semibold">Shell PATH</h3>
        <p className="mb-2 text-xs text-muted-foreground">
          {shell ? (
            <>
              Detected shell: <code>{shell}</code> — rc file: <code>{RC_DISPLAY[shell]}</code>.
            </>
          ) : (
            'Detecting your shell…'
          )}
        </p>
        <Button size="sm" onClick={handleInstallPath} disabled={!shell}>
          Install / re-install hook
        </Button>
        {pathMessage ? <p className="mt-2 text-sm text-muted-foreground">{pathMessage}</p> : null}
        {pathError ? <p className="mt-2 text-sm text-red">{pathError}</p> : null}
      </section>

      <section className="mb-6">
        <h3 className="mb-2 text-sm font-semibold">Onboarding</h3>
        <p className="mb-2 text-xs text-muted-foreground">
          Show the welcome dialog, migration prompt, and PATH banner again next launch.
        </p>
        <Button size="sm" variant="ghost" onClick={handleResetOnboarding}>
          Reset welcome and dismissal flags
        </Button>
        {resetMessage ? <p className="mt-2 text-sm text-muted-foreground">{resetMessage}</p> : null}
      </section>
    </main>
  )
}
