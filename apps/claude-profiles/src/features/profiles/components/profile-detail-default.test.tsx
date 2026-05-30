import type { DefaultEntry } from '@/lib/types'

import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { renderWithQuery } from '@/test/render-with-query'

import { DefaultProfileDetail } from './profile-detail-default'

vi.mock('@/lib/commands', async () => {
  const actual = await vi.importActual<typeof import('@/lib/commands')>('@/lib/commands')
  return {
    ...actual,
    profilePaths: vi.fn(async () => ({
      dataDir: '/Users/me/.claude',
      guiDataDir: '/Users/me/Library/Application Support/Claude',
      cliConfigDir: '/Users/me/.claude',
      guiLauncherPath: '/Applications/Claude.app',
      cliWrapperPath: null,
    })),
    getProfileUsage: vi.fn(async () => ({
      quota: {
        fiveHour: { utilization: 10, resetsAt: null },
        sevenDay: { utilization: 5, resetsAt: null },
        sevenDaySonnet: { utilization: 2, resetsAt: null },
      },
      quotaError: null,
      fetchedAt: '2099-01-01T00:00:00Z',
    })),
    checkDependencies: vi.fn(async () => ({
      claudeAppInstalled: true,
      claudeCliInstalled: true,
      localBinOnPath: true,
    })),
    openInFinder: vi.fn(async () => {}),
    openClaudeGui: vi.fn(async () => {}),
    copyToClipboard: vi.fn(async () => {}),
  }
})

function entry(overrides: Partial<DefaultEntry> = {}): DefaultEntry {
  return {
    id: 'default:claude',
    app: 'claude',
    name: 'Default',
    surfaces: { gui: true, cli: true },
    ...overrides,
  }
}

describe('DefaultProfileDetail', () => {
  it('renders the header without an Edit button', async () => {
    renderWithQuery(<DefaultProfileDetail entry={entry()} onMigrate={vi.fn()} />)
    await screen.findByText('Default')
    expect(screen.queryByRole('button', { name: /Edit/ })).toBeNull()
  })

  it('does not render a Recent Activity section', async () => {
    renderWithQuery(<DefaultProfileDetail entry={entry()} onMigrate={vi.fn()} />)
    await screen.findByText('Default')
    expect(screen.queryByText(/Recent activity/i)).toBeNull()
  })

  it('shows the migrate action and fires onMigrate when clicked', async () => {
    const onMigrate = vi.fn()
    renderWithQuery(<DefaultProfileDetail entry={entry()} onMigrate={onMigrate} />)
    const button = await screen.findByRole('button', { name: /Migrate this profile/i })
    await userEvent.setup().click(button)
    expect(onMigrate).toHaveBeenCalledTimes(1)
  })

  it('labels the CLI primary action as "Copy claude"', async () => {
    renderWithQuery(<DefaultProfileDetail entry={entry()} onMigrate={vi.fn()} />)
    const button = await screen.findByRole('button', { name: /Copy\s+claude\b/ })
    expect(button).toBeInTheDocument()
  })
})
