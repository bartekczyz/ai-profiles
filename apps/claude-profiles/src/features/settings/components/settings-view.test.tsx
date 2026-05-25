import type { ReactElement } from 'react'

import { invoke } from '@tauri-apps/api/core'
import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ThemeProvider, ToastProvider } from '@/design'
import { renderWithQuery } from '@/test/render-with-query'

import { SettingsView } from './settings-view'

function renderSettings(ui: ReactElement) {
  return renderWithQuery(
    <ThemeProvider defaultMode="system">
      <ToastProvider>{ui}</ToastProvider>
    </ThemeProvider>,
  )
}

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

const DEFAULT_EXISTING = {
  claudeDesktopPath: null,
  claudeCodePath: null,
  claudeDesktopSizeBytes: null,
  claudeCodeSizeBytes: null,
}

const DEFAULT_METADATA = {
  name: 'claude-profiles',
  version: '0.1.0',
  description: 'Test description',
  authors: ['Bartek Czyż <bartek@czyz.it>'],
  repository: null,
  homepage: null,
  license: 'MIT',
}

beforeEach(() => {
  mockInvoke.mockReset()
})

// NOTE: If a future change adds a new hook to SettingsView (or a new IPC call
// during the initial render), update primeInitialLoads to dispatch the new
// command name. The dispatch-by-name pattern avoids ordering coupling.
function primeInitialLoads({
  deps = HEALTHY_DEPS,
  backups = [] as Array<unknown>,
  state = DEFAULT_STATE,
  shell = 'zsh',
  existing = DEFAULT_EXISTING,
  metadata = DEFAULT_METADATA,
}: {
  deps?: unknown
  backups?: Array<unknown>
  state?: unknown
  shell?: string
  existing?: unknown
  metadata?: unknown
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
    if (command === 'detect_existing_claude_install') {
      return existing
    }
    if (command === 'get_app_metadata') {
      return metadata
    }
    throw new Error(`unexpected command in test: ${command}`)
  })
}

describe('SettingsView', () => {
  it('renders the app version in the footer row', async () => {
    primeInitialLoads()
    renderSettings(<SettingsView onClose={vi.fn()} onOpenMigration={vi.fn()} onOpenAbout={vi.fn()} />)
    expect(await screen.findByText(/claude-profiles v0\.1\.0/)).toBeInTheDocument()
  })

  it('renders the System status card with one row per dependency', async () => {
    primeInitialLoads()
    renderSettings(<SettingsView onClose={vi.fn()} onOpenMigration={vi.fn()} onOpenAbout={vi.fn()} />)
    await waitFor(() => expect(screen.getByText('Claude Desktop')).toBeInTheDocument())
    expect(screen.getByText('Claude Code CLI')).toBeInTheDocument()
    expect(screen.getByText('Shell PATH')).toBeInTheDocument()
  })

  it('shows the no-backups empty card when there are none', async () => {
    primeInitialLoads({ backups: [] })
    renderSettings(<SettingsView onClose={vi.fn()} onOpenMigration={vi.fn()} onOpenAbout={vi.fn()} />)
    expect(await screen.findByText(/No migration backups/)).toBeInTheDocument()
  })

  it('renders the Re-import action card when an install is detected', async () => {
    primeInitialLoads({
      existing: {
        claudeDesktopPath: '/Users/me/Library/Application Support/Claude',
        claudeCodePath: null,
        claudeDesktopSizeBytes: 248 * 1024 * 1024,
        claudeCodeSizeBytes: null,
      },
    })
    renderSettings(<SettingsView onClose={vi.fn()} onOpenMigration={vi.fn()} onOpenAbout={vi.fn()} />)
    expect(await screen.findByText(/Detected an existing Claude install/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Re-import/ })).toBeInTheDocument()
  })

  it('hides the Re-import action card when nothing is detected', async () => {
    primeInitialLoads()
    renderSettings(<SettingsView onClose={vi.fn()} onOpenMigration={vi.fn()} onOpenAbout={vi.fn()} />)
    await waitFor(() => expect(screen.getByText('Claude Desktop')).toBeInTheDocument())
    expect(screen.queryByText(/Detected an existing Claude install/)).not.toBeInTheDocument()
  })

  it('Re-import button calls onOpenMigration', async () => {
    primeInitialLoads({
      existing: {
        claudeDesktopPath: '/Users/me/Library/Application Support/Claude',
        claudeCodePath: null,
        claudeDesktopSizeBytes: null,
        claudeCodeSizeBytes: null,
      },
    })
    const onOpenMigration = vi.fn()
    renderSettings(<SettingsView onClose={vi.fn()} onOpenMigration={onOpenMigration} onOpenAbout={vi.fn()} />)
    await userEvent.setup().click(await screen.findByRole('button', { name: /Re-import/ }))
    expect(onOpenMigration).toHaveBeenCalled()
  })

  it('shows a confirmation dialog before resetting onboarding flags', async () => {
    primeInitialLoads()
    renderSettings(<SettingsView onClose={vi.fn()} onOpenMigration={vi.fn()} onOpenAbout={vi.fn()} />)
    await waitFor(() => expect(screen.getByText('Claude Desktop')).toBeInTheDocument())

    mockInvoke.mockReset()
    mockInvoke.mockImplementation(async (command: string) => {
      if (command === 'update_app_state') {
        return DEFAULT_STATE
      }
      throw new Error(`unexpected command in test: ${command}`)
    })

    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: /Reset onboarding flags/ }))

    expect(screen.getByRole('dialog', { name: /Reset onboarding state\?/ })).toBeInTheDocument()
    expect(mockInvoke).not.toHaveBeenCalled()

    await user.click(screen.getByRole('button', { name: 'Reset' }))

    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith('update_app_state', {
        patch: {
          welcomeShown: false,
          clearMigrationDismissed: true,
          clearPathBannerDismissed: true,
        },
      }),
    )
  })

  it('cancels reset when the user picks Cancel', async () => {
    primeInitialLoads()
    renderSettings(<SettingsView onClose={vi.fn()} onOpenMigration={vi.fn()} onOpenAbout={vi.fn()} />)
    await waitFor(() => expect(screen.getByText('Claude Desktop')).toBeInTheDocument())

    mockInvoke.mockReset()
    mockInvoke.mockImplementation(async (command: string) => {
      throw new Error(`unexpected command in test: ${command}`)
    })

    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: /Reset onboarding flags/ }))
    await user.click(screen.getByRole('button', { name: 'Cancel' }))

    expect(mockInvoke).not.toHaveBeenCalled()
    expect(screen.queryByRole('dialog', { name: /Reset onboarding state\?/ })).not.toBeInTheDocument()
  })

  it('appearance segmented control persists the theme via update_app_state', async () => {
    primeInitialLoads()
    renderSettings(<SettingsView onClose={vi.fn()} onOpenMigration={vi.fn()} onOpenAbout={vi.fn()} />)
    await waitFor(() => expect(screen.getByText('Claude Desktop')).toBeInTheDocument())

    mockInvoke.mockReset()
    mockInvoke.mockImplementation(async (command: string) => {
      if (command === 'update_app_state') {
        return { ...DEFAULT_STATE, themeMode: 'dark' }
      }
      throw new Error(`unexpected command in test: ${command}`)
    })

    await userEvent.setup().click(screen.getByRole('radio', { name: /Dark theme/ }))

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith('update_app_state', { patch: { themeMode: 'dark' } }))
    // The Dark radio is checked after the click — the segmented control
    // is the only source of truth for the chosen mode now that the
    // "Currently: …" helper line is gone.
    expect(screen.getByRole('radio', { name: /Dark theme/ })).toHaveAttribute('aria-checked', 'true')
  })
})
