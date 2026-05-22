import { invoke } from '@tauri-apps/api/core'
import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { renderWithQuery } from '@/test/render-with-query'

import { SettingsView } from './settings-view'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn().mockResolvedValue('0.1.0') }))

const mockInvoke = vi.mocked(invoke)

const HEALTHY_DEPS = {
  claudeAppInstalled: true,
  claudeCliInstalled: true,
  localBinOnPath: true,
}

const DEFAULT_STATE = {
  welcomeShown: true,
  migrationDismissedAt: null,
  pathBannerDismissedAt: null,
  themeMode: 'system' as const,
}

beforeEach(() => {
  mockInvoke.mockReset()
})

// NOTE: If a future change adds a fourth hook to SettingsView (or a new IPC call
// during the initial render), update primeInitialLoads to dispatch the new
// command name. The dispatch-by-name pattern avoids ordering coupling.
function primeInitialLoads({
  deps = HEALTHY_DEPS,
  backups = [] as Array<unknown>,
  state = DEFAULT_STATE,
  shell = 'zsh',
}: {
  deps?: unknown
  backups?: Array<unknown>
  state?: unknown
  shell?: string
} = {}) {
  mockInvoke.mockImplementation(async (command: string) => {
    if (command === 'check_dependencies') {
      return deps
    }
    if (command === 'list_migration_backups') {
      return backups
    }
    if (command === 'load_app_state') {
      return state
    }
    if (command === 'detect_shell') {
      return shell
    }
    throw new Error(`unexpected command in test: ${command}`)
  })
}

describe('SettingsView', () => {
  it('renders the app version from getVersion()', async () => {
    primeInitialLoads()
    renderWithQuery(<SettingsView onClose={vi.fn()} onOpenMigration={vi.fn()} />)
    expect(await screen.findByText(/claude-profiles 0\.1\.0/)).toBeInTheDocument()
  })

  it('shows green checks for all-installed dependencies', async () => {
    primeInitialLoads()
    renderWithQuery(<SettingsView onClose={vi.fn()} onOpenMigration={vi.fn()} />)
    await waitFor(() => expect(screen.getByText(/Claude Desktop:/)).toBeInTheDocument())
    expect(screen.getByText(/Claude Desktop: ✓ installed/)).toBeInTheDocument()
    expect(screen.getByText(/Claude Code CLI: ✓ installed/)).toBeInTheDocument()
  })

  it('shows the empty-state for migration backups when there are none', async () => {
    primeInitialLoads({ backups: [] })
    renderWithQuery(<SettingsView onClose={vi.fn()} onOpenMigration={vi.fn()} />)
    expect(await screen.findByText(/No migration backups/)).toBeInTheDocument()
  })

  it('opens migration dialog and clears prior dismissal when detect-and-import finds installs', async () => {
    primeInitialLoads()
    const onOpenMigration = vi.fn()
    renderWithQuery(<SettingsView onClose={vi.fn()} onOpenMigration={onOpenMigration} />)
    await waitFor(() => expect(screen.getByText(/Claude Desktop:/)).toBeInTheDocument())

    // After initial loads, swap to one-shot mocks for the action flow.
    mockInvoke.mockReset()
    mockInvoke.mockImplementation(async (command: string) => {
      if (command === 'detect_existing_claude_install') {
        return { claudeDesktopPath: '/x/Claude', claudeCodePath: '/x/.claude' }
      }
      if (command === 'update_app_state') {
        return DEFAULT_STATE
      }
      throw new Error(`unexpected command in test: ${command}`)
    })

    await userEvent.setup().click(screen.getByRole('button', { name: /Detect and import/ }))

    await waitFor(() => expect(onOpenMigration).toHaveBeenCalled())
    expect(mockInvoke).toHaveBeenCalledWith('update_app_state', {
      patch: { clearMigrationDismissed: true },
    })
  })

  it('shows the "no existing installs" message when detect returns empty', async () => {
    primeInitialLoads()
    const onOpenMigration = vi.fn()
    renderWithQuery(<SettingsView onClose={vi.fn()} onOpenMigration={onOpenMigration} />)
    await waitFor(() => expect(screen.getByText(/Claude Desktop:/)).toBeInTheDocument())

    mockInvoke.mockReset()
    mockInvoke.mockImplementation(async (command: string) => {
      if (command === 'detect_existing_claude_install') {
        return { claudeDesktopPath: null, claudeCodePath: null }
      }
      throw new Error(`unexpected command in test: ${command}`)
    })

    await userEvent.setup().click(screen.getByRole('button', { name: /Detect and import/ }))
    expect(await screen.findByText(/No existing Claude installs detected/i)).toBeInTheDocument()
    expect(onOpenMigration).not.toHaveBeenCalled()
  })

  it('reset button calls update_app_state with welcomeShown:false + clear flags', async () => {
    primeInitialLoads()
    renderWithQuery(<SettingsView onClose={vi.fn()} onOpenMigration={vi.fn()} />)
    await waitFor(() => expect(screen.getByText(/Claude Desktop:/)).toBeInTheDocument())

    mockInvoke.mockReset()
    mockInvoke.mockImplementation(async (command: string) => {
      if (command === 'update_app_state') {
        return DEFAULT_STATE
      }
      throw new Error(`unexpected command in test: ${command}`)
    })

    await userEvent.setup().click(screen.getByRole('button', { name: /Reset welcome and dismissal flags/ }))

    expect(mockInvoke).toHaveBeenCalledWith('update_app_state', {
      patch: {
        welcomeShown: false,
        clearMigrationDismissed: true,
        clearPathBannerDismissed: true,
      },
    })
    expect(await screen.findByText(/Onboarding flags reset/)).toBeInTheDocument()
  })
})
