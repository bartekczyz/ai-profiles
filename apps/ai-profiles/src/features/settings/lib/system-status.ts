import type { UpdaterStatus } from '@/features/updater/api/use-updater'
import type { PathHookOutcome, Shell } from '@/lib/types'

/**
 * The rc file the PATH hook goes in, per shell, as the user would write it.
 */
export const rcDisplay: Record<Shell, string> = {
  zsh: '~/.zshrc',
  bash: '~/.bashrc',
  fish: '~/.config/fish/config.fish',
}

/**
 * What the updater hookline's button does in the updater's current state.
 */
type UpdateAction = {
  /**
   * The button's label.
   */
  label: string
  /**
   * Whether the button is locked, because a check or an install is under way.
   */
  busy: boolean
  /**
   * Whether pressing it installs the available update and restarts, rather
   * than checking for one.
   */
  installs: boolean
}

/**
 * Pure: the line under the System card saying which shell was detected and
 * whether the PATH hook is in its rc file.
 */
export function shellHookStatus(shell: Shell | null, hookInstalled: boolean): string {
  if (shell === null) {
    return 'Detecting your shell…'
  }
  if (hookInstalled) {
    return `Detected ${shell} — hook installed in ${rcDisplay[shell]}.`
  }
  return `Detected ${shell} — hook not yet installed in ${rcDisplay[shell]}.`
}

/**
 * Pure: what to tell the user after installing the PATH hook into `shell`'s
 * rc file.
 */
export function hookInstallMessage(shell: Shell, outcome: PathHookOutcome): string {
  if (outcome.outcome === 'alreadyInstalled') {
    return `${rcDisplay[shell]} already has the hook.`
  }
  return `Updated ${rcDisplay[shell]}. Open a new terminal to pick it up.`
}

/**
 * Pure: the updater hookline's button: install and restart once an update is
 * available, check for one otherwise; locked while either is under way.
 */
export function updateAction(status: UpdaterStatus): UpdateAction {
  const installs = status.kind === 'available'
  return {
    label: installs ? 'Restart and install' : 'Check now',
    busy: status.kind === 'checking' || status.kind === 'installing',
    installs,
  }
}
