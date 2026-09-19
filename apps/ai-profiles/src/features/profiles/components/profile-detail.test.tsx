import type { Profile, ProfilePaths } from '@/lib/types'

import { useState } from 'react'

import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { copyToClipboard, openInFinder, openProfileInApp, profilePaths, touchProfileLastUsed } from '@/lib/commands'
import { renderWithQuery } from '@/test/render-with-query'

import { DeleteProfileDialog } from './delete-profile-dialog'
import { ProfileDetail } from './profile-detail'

vi.mock('@/lib/commands', async () => {
  const actual = await vi.importActual<typeof import('@/lib/commands')>('@/lib/commands')
  return {
    ...actual,
    profilePaths: vi.fn(),
    openInFinder: vi.fn(async () => {}),
    openProfileInApp: vi.fn(async () => {}),
    copyToClipboard: vi.fn(async () => {}),
    touchProfileLastUsed: vi.fn(async () => {}),
    getProfileUsage: vi.fn(async () => ({
      quota: {
        primary: { utilization: 10, resetsAt: null },
        secondary: { utilization: 5, resetsAt: null },
        secondaryExtra: { utilization: 2, resetsAt: null },
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
  }
})

const guiDataDir = '/Users/ada/Library/Application Support/ai-profiles/profiles/p1/gui-data'
const cliConfigDir = '/Users/ada/Library/Application Support/ai-profiles/profiles/p1/cli-config'
const guiLauncherPath = '/Applications/Claude (Work).app'
const cliWrapperPath = '/Users/ada/.local/bin/claude-work'

function paths(overrides: Partial<ProfilePaths> = {}): ProfilePaths {
  return {
    dataDir: '/Users/ada/Library/Application Support/ai-profiles/profiles/p1',
    guiDataDir,
    cliConfigDir,
    guiLauncherPath,
    cliWrapperPath,
    ...overrides,
  }
}

function profile(overrides: Partial<Profile> = {}): Profile {
  return {
    id: 'p1',
    app: 'claude',
    name: 'Work',
    slug: 'work',
    color: '#d97757',
    createdAt: '2026-01-01T00:00:00Z',
    surfaces: { gui: true, cli: true },
    distinctDockIcon: false,
    lastUsedAt: null,
    ...overrides,
  }
}

type RenderOverrides = {
  profile?: Profile
  shortcutsEnabled?: boolean
  onEdit?: () => void
  onDelete?: () => void
}

function renderDetail(overrides: RenderOverrides = {}) {
  const { profile: value = profile(), shortcutsEnabled = true, onEdit = vi.fn(), onDelete = vi.fn() } = overrides
  return renderWithQuery(
    <ProfileDetail shortcutsEnabled={shortcutsEnabled} profile={value} onEdit={onEdit} onDelete={onDelete} />,
  )
}

async function openMenu() {
  const user = userEvent.setup()
  await user.click(await screen.findByRole('button', { name: 'More actions' }))
  await screen.findByRole('menu')
  return user
}

beforeEach(() => {
  vi.mocked(profilePaths).mockResolvedValue(paths())
  vi.mocked(openInFinder).mockClear()
  vi.mocked(openProfileInApp).mockClear()
  vi.mocked(copyToClipboard).mockClear()
  vi.mocked(touchProfileLastUsed).mockClear()
})

describe('ProfileDetail — overflow menu', () => {
  it('opens the menu from the header trigger', async () => {
    renderDetail()
    await openMenu()
    expect(screen.getAllByRole('menuitem').length).toBeGreaterThan(0)
  })

  it.each([
    ['Desktop app data', () => guiDataDir],
    ['Launcher', () => guiLauncherPath],
    ['CLI config', () => cliConfigDir],
    ['CLI wrapper', () => cliWrapperPath],
  ])('reveals %s at its resolved path', async (label, expectedPath) => {
    renderDetail()
    const user = await openMenu()
    await user.click(screen.getByRole('menuitem', { name: new RegExp(label) }))
    expect(openInFinder).toHaveBeenCalledWith(expectedPath())
  })

  it('does not offer a destination the profile has no path for', async () => {
    vi.mocked(profilePaths).mockResolvedValue(paths({ cliWrapperPath: null }))
    renderDetail()
    await openMenu()
    expect(screen.queryByRole('menuitem', { name: /CLI wrapper/ })).toBeNull()
    expect(screen.getByRole('menuitem', { name: /CLI config/ })).toBeInTheDocument()
  })

  it('surfaces an error when a reveal fails', async () => {
    vi.mocked(openInFinder).mockRejectedValueOnce(new Error('no such directory'))
    renderDetail()
    const user = await openMenu()
    await user.click(screen.getByRole('menuitem', { name: /Desktop app data/ }))
    expect(await screen.findByRole('alert')).toHaveTextContent('no such directory')
  })

  it('opens the delete confirmation from the menu', async () => {
    function Harness() {
      const [deleting, setDeleting] = useState(false)
      return (
        <>
          <ProfileDetail shortcutsEnabled profile={profile()} onEdit={vi.fn()} onDelete={() => setDeleting(true)} />
          <DeleteProfileDialog
            open={deleting}
            profile={profile()}
            onClose={() => setDeleting(false)}
            onConfirm={vi.fn().mockResolvedValue(undefined)}
          />
        </>
      )
    }
    renderWithQuery(<Harness />)
    const user = await openMenu()
    expect(screen.queryByRole('dialog')).toBeNull()
    await user.click(screen.getByRole('menuitem', { name: /Delete profile/ }))
    expect(await screen.findByRole('dialog')).toBeInTheDocument()
  })

  it('dismisses the menu with Escape', async () => {
    renderDetail()
    const user = await openMenu()
    await user.keyboard('{Escape}')
    await waitFor(() => {
      expect(screen.queryByRole('menu')).toBeNull()
    })
  })

  it('requests an edit from the header without opening the menu', async () => {
    const onEdit = vi.fn()
    renderDetail({ onEdit })
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Edit' }))
    expect(onEdit).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('menu')).toBeNull()
  })
})

describe('ProfileDetail — reveal shortcuts', () => {
  it.each([
    ['1', () => guiDataDir],
    ['2', () => guiLauncherPath],
    ['3', () => cliConfigDir],
    ['4', () => cliWrapperPath],
  ])('⌥%s reveals its destination without opening the menu', async (digit, expectedPath) => {
    renderDetail()
    const user = userEvent.setup()
    await screen.findByRole('button', { name: 'More actions' })
    await user.keyboard(`{Alt>}${digit}{/Alt}`)
    expect(openInFinder).toHaveBeenCalledWith(expectedPath())
  })

  it('does not fire the wrapper shortcut when the profile has no wrapper', async () => {
    vi.mocked(profilePaths).mockResolvedValue(paths({ cliWrapperPath: null }))
    renderDetail()
    const user = userEvent.setup()
    await screen.findByRole('button', { name: 'More actions' })
    await user.keyboard('{Alt>}4{/Alt}')
    expect(openInFinder).not.toHaveBeenCalled()
  })
})

describe('ProfileDetail — surfaces', () => {
  const command = 'claude-work'

  async function findLaunchControl() {
    return screen.findByRole('button', { name: 'Open' })
  }

  async function findCopyControl() {
    return screen.findByRole('button', { name: command })
  }

  /**
   * The launch runs through a TanStack mutation, which hands the mutation
   * context to its function as a second argument — read the profile id off
   * each call rather than pinning the whole signature.
   */
  function launchedProfileIds(): Array<string> {
    return vi.mocked(openProfileInApp).mock.calls.map(([profileId]) => profileId)
  }

  it('launches the desktop app from the desktop row', async () => {
    renderDetail()
    const user = userEvent.setup()
    await user.click(await findLaunchControl())
    expect(launchedProfileIds()).toEqual(['p1'])
  })

  it('launches the desktop app from the keyboard', async () => {
    renderDetail()
    const user = userEvent.setup()
    await findLaunchControl()
    await user.keyboard('{Enter}')
    await waitFor(() => {
      expect(launchedProfileIds()).toEqual(['p1'])
    })
  })

  it('copies the command from the token and confirms on it', async () => {
    renderDetail()
    const user = userEvent.setup()
    const token = await findCopyControl()
    await user.click(token)
    expect(copyToClipboard).toHaveBeenCalledWith(command)
    await waitFor(() => {
      expect(token).toHaveAttribute('data-copied', 'true')
    })
  })

  it('copies the command from the keyboard and confirms on the same token', async () => {
    renderDetail()
    const user = userEvent.setup()
    const token = await findCopyControl()
    await user.keyboard('{Meta>}c{/Meta}')
    await waitFor(() => {
      expect(copyToClipboard).toHaveBeenCalledWith(command)
    })
    await waitFor(() => {
      expect(token).toHaveAttribute('data-copied', 'true')
    })
  })

  it('stamps last-used when the command is copied', async () => {
    renderDetail()
    const user = userEvent.setup()
    await user.click(await findCopyControl())
    await waitFor(() => {
      expect(touchProfileLastUsed).toHaveBeenCalledWith('p1')
    })
  })

  it('offers no launch route when the desktop surface is off', async () => {
    renderDetail({ profile: profile({ surfaces: { gui: false, cli: true } }) })
    const user = userEvent.setup()
    await findCopyControl()
    expect(screen.queryByRole('button', { name: 'Open' })).toBeNull()
    await user.keyboard('{Enter}')
    expect(openProfileInApp).not.toHaveBeenCalled()
  })

  it('offers no copy route when the CLI surface is off', async () => {
    renderDetail({ profile: profile({ surfaces: { gui: true, cli: false } }) })
    const user = userEvent.setup()
    await findLaunchControl()
    expect(screen.queryByRole('button', { name: command })).toBeNull()
    await user.keyboard('{Meta>}c{/Meta}')
    expect(copyToClipboard).not.toHaveBeenCalled()
  })

  it('states each surface for itself rather than adding a merged line', async () => {
    renderDetail({ profile: profile({ surfaces: { gui: true, cli: false } }) })
    await findLaunchControl()
    expect(screen.queryAllByRole('status')).toHaveLength(0)
  })

  it('offers neither route and exactly one instruction when both surfaces are off', async () => {
    renderDetail({ profile: profile({ surfaces: { gui: false, cli: false } }) })
    await screen.findByRole('button', { name: 'More actions' })
    expect(screen.queryByRole('button', { name: 'Open' })).toBeNull()
    expect(screen.queryByRole('button', { name: command })).toBeNull()
    expect(screen.getAllByRole('status')).toHaveLength(1)
  })

  it('surfaces an error when a launch fails', async () => {
    vi.mocked(openProfileInApp).mockRejectedValueOnce(new Error('launcher is missing'))
    renderDetail()
    const user = userEvent.setup()
    await user.click(await findLaunchControl())
    expect(await screen.findByRole('alert')).toHaveTextContent('launcher is missing')
  })

  it('surfaces an error when a copy fails and withholds the confirmation', async () => {
    vi.mocked(copyToClipboard).mockRejectedValueOnce(new Error('clipboard unavailable'))
    renderDetail()
    const user = userEvent.setup()
    const token = await findCopyControl()
    await user.click(token)
    expect(await screen.findByRole('alert')).toHaveTextContent('clipboard unavailable')
    expect(token).toHaveAttribute('data-copied', 'false')
  })

  it('holds both shortcuts while something covers the pane', async () => {
    renderDetail({ shortcutsEnabled: false })
    const user = userEvent.setup()
    await findLaunchControl()
    await user.keyboard('{Enter}')
    await user.keyboard('{Meta>}c{/Meta}')
    expect(openProfileInApp).not.toHaveBeenCalled()
    expect(copyToClipboard).not.toHaveBeenCalled()
  })
})

describe('ProfileDetail — profile explainer', () => {
  it('reveals the explanation on demand and dismisses it with Escape', async () => {
    renderDetail()
    const trigger = await screen.findByRole('button', { name: 'About this profile' })
    expect(screen.queryByRole('dialog')).toBeNull()

    const user = userEvent.setup()
    await user.click(trigger)
    expect(await screen.findByRole('dialog')).toBeInTheDocument()

    await user.keyboard('{Escape}')
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull()
    })
  })
})
