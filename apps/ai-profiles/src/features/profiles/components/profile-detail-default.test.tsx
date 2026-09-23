import type { AppState, DefaultEntry, ProfilePaths } from '@/lib/types'

import { QueryClient } from '@tanstack/react-query'
import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'
import {
  copyToClipboard,
  loadAppState,
  openDefaultGui,
  openInFinder,
  profilePaths,
  updateAppState,
} from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'
import { renderWithQuery } from '@/test/render-with-query'

import { DefaultProfileDetail } from './profile-detail-default'

vi.mock('@/lib/commands', async () => {
  const actual = await vi.importActual<typeof import('@/lib/commands')>('@/lib/commands')
  return {
    ...actual,
    listSessions: vi.fn(async () => ({ sessions: [], repairCount: 0 })),
    profilePaths: vi.fn(),
    getProfileUsage: vi.fn(async () => ({
      quota: {
        primary: { utilization: 10, resetsAt: null },
        secondary: { utilization: 5, resetsAt: null },
        scopedWeekly: [{ utilization: 2, resetsAt: null }],
      },
      quotaError: null,
      fetchedAt: '2099-01-01T00:00:00Z',
    })),
    checkDependencies: vi.fn(async () => ({
      apps: {
        claude: { guiInstalled: true, cliInstalled: true },
        codex: { guiInstalled: false, cliInstalled: false },
      },
      localBinOnPath: true,
    })),
    openInFinder: vi.fn(async () => {}),
    openDefaultGui: vi.fn(async () => {}),
    copyToClipboard: vi.fn(async () => {}),
    loadAppState: vi.fn(),
    updateAppState: vi.fn(),
  }
})

const appState: AppState = {
  welcomeShown: true,
  migrationDismissedAt: null,
  pathBannerDismissedAt: null,
  themeMode: 'system',
  selectedEntryId: null,
  dockIconAcknowledgedAt: null,
  defaultProfileNames: {},
}

const guiDataDir = '/Users/ada/Library/Application Support/Claude'
const cliConfigDir = '/Users/ada/.claude'
const guiLauncherPath = '/Applications/Claude.app'

function paths(overrides: Partial<ProfilePaths> = {}): ProfilePaths {
  return {
    dataDir: cliConfigDir,
    guiDataDir,
    cliConfigDir,
    guiLauncherPath,
    cliWrapperPath: null,
    ...overrides,
  }
}

function entry(overrides: Partial<DefaultEntry> = {}): DefaultEntry {
  return {
    id: 'default:claude',
    app: 'claude',
    name: 'Default',
    customName: null,
    surfaces: { gui: true, cli: true },
    ...overrides,
  }
}

type RenderOverrides = {
  entry?: DefaultEntry
  onMigrate?: () => void
}

function renderDefault(overrides: RenderOverrides = {}) {
  const { entry: value = entry(), onMigrate = vi.fn() } = overrides
  // The app shell loads app state before any pane mounts, so the pane never
  // suspends on it. Seed it the same way, or the whole pane suspends and the
  // early-click desktop test runs away.
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Number.POSITIVE_INFINITY } } })
  client.setQueryData(queryKeys.appState, appState)
  return renderWithQuery(
    <ToastProvider>
      <DefaultProfileDetail entry={value} onMigrate={onMigrate} />
    </ToastProvider>,
    { client },
  )
}

async function openMenu(overrides: RenderOverrides = {}) {
  renderDefault(overrides)
  const user = userEvent.setup()
  await user.click(await screen.findByRole('button', { name: 'More actions' }))
  await screen.findByRole('menu')
  return user
}

beforeEach(() => {
  vi.mocked(profilePaths).mockReset()
  vi.mocked(profilePaths).mockResolvedValue(paths())
  vi.mocked(openInFinder).mockClear()
  vi.mocked(openDefaultGui).mockClear()
  vi.mocked(copyToClipboard).mockClear()
  vi.mocked(loadAppState).mockResolvedValue(appState)
  vi.mocked(updateAppState).mockReset()
  vi.mocked(updateAppState).mockResolvedValue(appState)
})

describe('DefaultProfileDetail — absent capabilities', () => {
  it('offers no route to editing the entry', async () => {
    renderDefault()
    await screen.findByRole('button', { name: 'More actions' })
    expect(screen.queryByRole('button', { name: 'Edit' })).toBeNull()
  })

  it('offers no route to deleting the entry', async () => {
    await openMenu()
    expect(screen.queryByRole('menuitem', { name: /Delete/ })).toBeNull()
  })

  it('offers no wrapper destination, having no wrapper', async () => {
    await openMenu()
    expect(screen.queryByRole('menuitem', { name: /CLI wrapper/ })).toBeNull()
  })

  it('leaves the wrapper shortcut dead, there being no wrapper to reveal', async () => {
    renderDefault()
    const user = userEvent.setup()
    await screen.findByRole('button', { name: 'More actions' })
    await user.keyboard('{Alt>}4{/Alt}')
    expect(openInFinder).not.toHaveBeenCalled()
  })
})

describe('DefaultProfileDetail — terminal', () => {
  it('copies the bare CLI binary rather than a per-profile wrapper', async () => {
    renderDefault()
    const token = await screen.findByRole('button', { name: 'claude' })
    await userEvent.setup().click(token)
    expect(copyToClipboard).toHaveBeenCalledWith('claude')
    await waitFor(() => {
      expect(token).toHaveAttribute('data-copied', 'true')
    })
  })

  it('copies the Codex binary for a Codex default entry', async () => {
    renderDefault({ entry: entry({ id: 'default:codex', app: 'codex' }) })
    await userEvent.setup().click(await screen.findByRole('button', { name: 'codex' }))
    expect(copyToClipboard).toHaveBeenCalledWith('codex')
  })

  it('offers no copy route when the stock CLI is not installed', async () => {
    renderDefault({ entry: entry({ surfaces: { gui: true, cli: false } }) })
    await screen.findByRole('button', { name: 'Open' })
    expect(screen.queryByRole('button', { name: 'claude' })).toBeNull()
  })
})

describe('DefaultProfileDetail — desktop', () => {
  it('launches the stock app against its own data directory', async () => {
    renderDefault()
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Open' }))
    await waitFor(() => {
      expect(openDefaultGui).toHaveBeenCalledWith('claude', guiDataDir)
    })
  })

  it('launches even when clicked before the paths have resolved', async () => {
    let resolvePaths: (value: ProfilePaths) => void = () => {}
    vi.mocked(profilePaths).mockReturnValue(
      new Promise<ProfilePaths>((resolve) => {
        resolvePaths = resolve
      }),
    )
    renderDefault()

    // The control is live from the first paint, before the query settles.
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Open' }))
    resolvePaths(paths())

    await waitFor(() => {
      expect(openDefaultGui).toHaveBeenCalledWith('claude', guiDataDir)
    })
  })

  it('explains itself rather than failing silently when the app is not installed', async () => {
    vi.mocked(profilePaths).mockResolvedValue(paths({ guiLauncherPath: null }))
    renderDefault()
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Open' }))
    expect(await screen.findByRole('alert')).toBeInTheDocument()
    expect(openDefaultGui).not.toHaveBeenCalled()
  })
})

describe('DefaultProfileDetail — import', () => {
  it('opens the migration flow from the header', async () => {
    const onMigrate = vi.fn()
    renderDefault({ onMigrate })
    await userEvent.setup().click(await screen.findByRole('button', { name: /^Import/ }))
    expect(onMigrate).toHaveBeenCalledTimes(1)
  })

  it('offers the import action without waiting for the paths to resolve', async () => {
    vi.mocked(profilePaths).mockReturnValue(new Promise<ProfilePaths>(() => {}))
    renderDefault()
    expect(await screen.findByRole('button', { name: /^Import/ })).toBeInTheDocument()
  })
})

describe('DefaultProfileDetail — reveal destinations', () => {
  it.each([
    ['Desktop app data', () => guiDataDir],
    ['Launcher', () => guiLauncherPath],
    ['CLI config', () => cliConfigDir],
  ])('reveals %s at its resolved stock path', async (label, expectedPath) => {
    const user = await openMenu()
    await user.click(screen.getByRole('menuitem', { name: new RegExp(label) }))
    expect(openInFinder).toHaveBeenCalledWith(expectedPath())
  })

  it.each([
    ['1', () => guiDataDir],
    ['2', () => guiLauncherPath],
    ['3', () => cliConfigDir],
  ])('⌥%s reveals its destination without opening the menu', async (digit, expectedPath) => {
    renderDefault()
    const user = userEvent.setup()
    await screen.findByRole('button', { name: 'More actions' })
    await user.keyboard(`{Alt>}${digit}{/Alt}`)
    expect(openInFinder).toHaveBeenCalledWith(expectedPath())
  })

  it('drops the launcher destination when the stock app is not installed', async () => {
    vi.mocked(profilePaths).mockResolvedValue(paths({ guiLauncherPath: null }))
    const user = await openMenu()
    expect(screen.queryByRole('menuitem', { name: /Launcher/ })).toBeNull()
    await user.keyboard('{Escape}')
    await waitFor(() => {
      expect(screen.queryByRole('menu')).toBeNull()
    })
    await user.keyboard('{Alt>}2{/Alt}')
    expect(openInFinder).not.toHaveBeenCalled()
  })
})

describe('DefaultProfileDetail — rename', () => {
  it('shows the stock label until renamed', async () => {
    renderDefault()
    expect(await screen.findByRole('heading', { name: 'Claude' })).toBeInTheDocument()
    expect(screen.getByText('stock install')).toBeInTheDocument()
  })

  it('shows a custom name, keeping the app named in the sub-line', async () => {
    renderDefault({ entry: entry({ name: 'Personal', customName: 'Personal' }) })
    expect(await screen.findByRole('heading', { name: 'Personal' })).toBeInTheDocument()
    expect(screen.getByText('Claude · stock install')).toBeInTheDocument()
  })

  it('saves a trimmed name for this app from the Rename dialog', async () => {
    const user = await openMenu()
    await user.click(screen.getByRole('menuitem', { name: 'Rename…' }))
    await user.type(await screen.findByRole('textbox', { name: 'Name' }), '  Personal  ')
    await user.click(screen.getByRole('button', { name: /Save/ }))
    await waitFor(() => {
      // mutationFn also receives TanStack's context argument; only the patch matters.
      expect(vi.mocked(updateAppState).mock.calls[0]?.[0]).toEqual({
        defaultProfileName: { app: 'claude', name: 'Personal' },
      })
    })
  })

  it('offers Reset name once renamed, restoring the stock label', async () => {
    const user = await openMenu({ entry: entry({ name: 'Personal', customName: 'Personal' }) })
    await user.click(screen.getByRole('menuitem', { name: 'Rename…' }))
    await user.click(await screen.findByRole('button', { name: 'Reset name' }))
    await waitFor(() => {
      // mutationFn also receives TanStack's context argument; only the patch matters.
      expect(vi.mocked(updateAppState).mock.calls[0]?.[0]).toEqual({
        defaultProfileName: { app: 'claude', name: '' },
      })
    })
  })
})
