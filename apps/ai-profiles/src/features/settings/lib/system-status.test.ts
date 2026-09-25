import type { Update } from '@tauri-apps/plugin-updater'

import { describe, expect, it } from 'vitest'

import { hookInstallMessage, shellHookStatus, updateAction } from './system-status'

describe('shellHookStatus', () => {
  it('says the shell is still being detected', () => {
    expect(shellHookStatus(null, false)).toBe('Detecting your shell…')
  })

  it('names the rc file the hook is installed in', () => {
    expect(shellHookStatus('fish', true)).toBe('Detected fish — hook installed in ~/.config/fish/config.fish.')
  })

  it('names the rc file the hook is missing from', () => {
    expect(shellHookStatus('zsh', false)).toBe('Detected zsh — hook not yet installed in ~/.zshrc.')
  })
})

describe('hookInstallMessage', () => {
  it('says the hook was already there', () => {
    expect(hookInstallMessage('bash', { outcome: 'alreadyInstalled', rcPath: '/Users/me/.bashrc' })).toBe(
      '~/.bashrc already has the hook.',
    )
  })

  it('asks for a new terminal after installing the hook', () => {
    const outcome = { outcome: 'installed', rcPath: '/Users/me/.zshrc', backupPath: '/Users/me/.zshrc.bak' } as const
    expect(hookInstallMessage('zsh', outcome)).toBe('Updated ~/.zshrc. Open a new terminal to pick it up.')
  })
})

describe('updateAction', () => {
  it('checks for an update when none is known', () => {
    expect(updateAction({ kind: 'up-to-date' })).toEqual({ label: 'Check now', busy: false, installs: false })
  })

  it('installs and restarts once an update is available', () => {
    const update = { version: '9.9.9' } as Update
    expect(updateAction({ kind: 'available', update })).toEqual({
      label: 'Restart and install',
      busy: false,
      installs: true,
    })
  })

  it('locks while a check or an install is under way', () => {
    expect(updateAction({ kind: 'checking' }).busy).toBe(true)
    expect(updateAction({ kind: 'installing' }).busy).toBe(true)
  })

  it('offers another check after an error', () => {
    expect(updateAction({ kind: 'error', message: 'offline' })).toEqual({
      label: 'Check now',
      busy: false,
      installs: false,
    })
  })
})
