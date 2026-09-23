import type { Session, SessionList } from '@/lib/types'

import { screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { listSessions } from '@/lib/commands'
import { renderWithQuery } from '@/test/render-with-query'

import { SessionsPanel } from './sessions-panel'

vi.mock('@/lib/commands', () => ({ listSessions: vi.fn() }))

/**
 * A session with every optional field empty, overridden per case.
 */
function makeSession(overrides: Partial<Session> = {}): Session {
  return {
    id: 's1',
    kind: 'cli',
    title: null,
    cwd: null,
    lastPrompt: null,
    lastUsedAt: '2026-09-01T10:00:00Z',
    archived: false,
    state: 'idle',
    needsRepair: false,
    unmovableReason: null,
    ...overrides,
  }
}

/**
 * Makes the next listing resolve with `sessions`.
 */
function mockSessions(sessions: Array<Session>) {
  vi.mocked(listSessions).mockResolvedValue({ sessions, repairCount: 0 } satisfies SessionList)
}

/**
 * The titles of the listed rows, top to bottom.
 */
function rowTitles(): Array<string> {
  const list = screen.getByRole('list', { name: 'Sessions' })
  return within(list)
    .getAllByRole('listitem')
    .map((row) => row.getAttribute('aria-label') ?? '')
}

/**
 * Renders the panel and waits for the first rows.
 */
async function renderPanel() {
  const user = userEvent.setup()
  const result = renderWithQuery(<SessionsPanel profileId="p1" app="claude" />)
  await screen.findByRole('list', { name: 'Sessions' })
  return { ...result, user }
}

const mixed = [
  makeSession({ id: 'a', kind: 'cli', title: 'Refactor the parser', lastUsedAt: '2026-09-03T10:00:00Z' }),
  makeSession({ id: 'b', kind: 'desktop', title: 'Plan the launch', lastUsedAt: '2026-09-02T10:00:00Z' }),
  makeSession({ id: 'c', kind: 'cli', title: 'Fix the build', lastUsedAt: '2026-09-01T10:00:00Z' }),
  makeSession({ id: 'd', kind: 'cli', title: 'Old parser work', archived: true }),
]

beforeEach(() => {
  vi.mocked(listSessions).mockReset()
})

describe('SessionsPanel', () => {
  it('lists the profile’s active sessions, most recently used first', async () => {
    mockSessions(mixed)
    await renderPanel()
    expect(listSessions).toHaveBeenCalledWith('p1')
    expect(rowTitles()).toEqual(['Refactor the parser', 'Plan the launch', 'Fix the build'])
  })

  it('narrows the rows as the search is typed', async () => {
    mockSessions(mixed)
    const { user } = await renderPanel()
    await user.type(screen.getByRole('searchbox'), 'parser')
    expect(rowTitles()).toEqual(['Refactor the parser'])
  })

  it('shows archived sessions on the Archived tab', async () => {
    mockSessions(mixed)
    const { user } = await renderPanel()
    await user.click(screen.getByRole('tab', { name: /Archived/ }))
    expect(rowTitles()).toEqual(['Old parser work'])
  })

  it('offers the kind filter only when the tab mixes both kinds', async () => {
    mockSessions(mixed)
    const { user } = await renderPanel()
    await user.click(screen.getByRole('radio', { name: 'Desktop' }))
    expect(rowTitles()).toEqual(['Plan the launch'])

    // The Archived tab holds only CLI sessions: the filter goes away, and the
    // Desktop choice made on the other tab no longer hides anything.
    await user.click(screen.getByRole('tab', { name: /Archived/ }))
    expect(screen.queryByRole('radiogroup')).toBeNull()
    expect(rowTitles()).toEqual(['Old parser work'])
  })

  it('reverses the order when the sort is toggled', async () => {
    mockSessions(mixed)
    const { user } = await renderPanel()
    await user.click(screen.getByRole('button', { name: /Last used/ }))
    expect(rowTitles()).toEqual(['Fix the build', 'Plan the launch', 'Refactor the parser'])
    await user.click(screen.getByRole('button', { name: /Last used/ }))
    expect(rowTitles()).toEqual(['Refactor the parser', 'Plan the launch', 'Fix the build'])
  })

  it('keeps the search across tabs and clears it from the no-match state', async () => {
    mockSessions(mixed)
    const { user } = await renderPanel()
    await user.type(screen.getByRole('searchbox'), 'launch')
    await user.click(screen.getByRole('tab', { name: /Archived/ }))
    expect(screen.queryByRole('list', { name: 'Sessions' })).toBeNull()

    await user.click(screen.getByRole('button', { name: 'Clear search' }))
    expect(screen.getByRole('searchbox')).toHaveValue('')
    expect(rowTitles()).toEqual(['Old parser work'])
  })

  it('names an untitled session by its fallback', async () => {
    mockSessions([makeSession({ id: 'x', title: null, cwd: '/Users/ada/Developer/app' })])
    await renderPanel()
    expect(rowTitles()).toEqual(['Untitled session'])
  })

  it('shows a listing error inline and refetches on Retry', async () => {
    vi.mocked(listSessions)
      .mockRejectedValueOnce({ kind: 'Io', message: 'codex app-server exited' })
      .mockResolvedValueOnce({ sessions: mixed, repairCount: 0 })
    const user = userEvent.setup()
    renderWithQuery(<SessionsPanel profileId="default:codex" app="codex" />)

    expect(await screen.findByRole('alert')).toHaveTextContent('codex app-server exited')
    await user.click(screen.getByRole('button', { name: 'Retry' }))
    await screen.findByRole('list', { name: 'Sessions' })
    expect(listSessions).toHaveBeenCalledTimes(2)
    expect(listSessions).toHaveBeenLastCalledWith('default:codex')
    expect(screen.queryByRole('alert')).toBeNull()
  })
})
