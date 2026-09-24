import type { Profile, Session, SessionList, SidebarEntry } from '@/lib/types'

import { act, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'
import { useSidebarEntries } from '@/features/profiles/api/use-sidebar-entries'
import { archiveSession, checkSessionAction, listSessions, moveSession, planSessionMove } from '@/lib/commands'
import { makeRetryingClient, renderWithQuery } from '@/test/render-with-query'

import { makeSession } from '../test/make-session'
import { SessionsPanel } from './sessions-panel'

vi.mock('@/lib/commands', () => ({
  archiveSession: vi.fn(),
  checkSessionAction: vi.fn(),
  checkSessionRepair: vi.fn(),
  listSessions: vi.fn(),
  moveSession: vi.fn(),
  planSessionMove: vi.fn(),
  repairSessions: vi.fn(),
  restoreSession: vi.fn(),
}))

vi.mock('@/features/profiles/api/use-sidebar-entries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/features/profiles/api/use-sidebar-entries')>()),
  useSidebarEntries: vi.fn(),
}))

/**
 * A managed profile's sidebar entry.
 */
function managedEntry(id: string, name: string, app: Profile['app']): SidebarEntry {
  return {
    kind: 'managed',
    profile: {
      id,
      app,
      name,
      slug: name.toLowerCase(),
      color: '#123456',
      createdAt: '2026-09-01T10:00:00Z',
      surfaces: { gui: true, cli: true },
      distinctDockIcon: false,
      lastUsedAt: null,
    },
  }
}

/**
 * The sidebar: Claude's stock install and three profiles, one of them Codex.
 */
const sidebarEntries: Array<SidebarEntry> = [
  {
    kind: 'default',
    entry: {
      id: 'default:claude',
      app: 'claude',
      name: 'Claude',
      customName: null,
      surfaces: { gui: true, cli: true },
    },
  },
  managedEntry('p1', 'Work', 'claude'),
  managedEntry('p2', 'Personal', 'claude'),
  managedEntry('p3', 'Chat', 'codex'),
]

/**
 * Makes the next listing resolve with `sessions`.
 */
function mockSessions(sessions: Array<Session>) {
  vi.mocked(listSessions).mockResolvedValue({ sessions, repairCount: 0 } satisfies SessionList)
}

/**
 * The listed rows, top to bottom.
 */
function rows(): Array<HTMLElement> {
  return within(screen.getByRole('list', { name: 'Sessions' })).getAllByRole('listitem')
}

/**
 * Asserts the list shows one row per title, in this order.
 */
function expectRows(titles: Array<string>) {
  expect(rows().map((item) => item.textContent)).toEqual(titles.map((title) => expect.stringContaining(title)))
}

/**
 * Renders the panel and waits for the first rows.
 */
async function renderPanel() {
  const user = userEvent.setup()
  const result = renderWithQuery(
    <ToastProvider>
      <SessionsPanel profileId="p1" app="claude" />
    </ToastProvider>,
  )
  await screen.findByRole('list', { name: 'Sessions' })
  return { ...result, user }
}

/**
 * Three active sessions of both kinds, newest first, and an archived one.
 */
const mixed = [
  makeSession({ id: 'a', kind: 'cli', title: 'Refactor the parser', lastUsedAt: '2026-09-03T10:00:00Z' }),
  makeSession({ id: 'b', kind: 'desktop', title: 'Plan the launch', lastUsedAt: '2026-09-02T10:00:00Z' }),
  makeSession({ id: 'c', kind: 'cli', title: 'Fix the build', lastUsedAt: '2026-09-01T10:00:00Z' }),
  makeSession({ id: 'd', kind: 'cli', title: 'Old parser work', archived: true }),
]

beforeEach(() => {
  vi.mocked(listSessions).mockReset()
  vi.mocked(archiveSession).mockReset().mockResolvedValue(undefined)
  vi.mocked(checkSessionAction).mockReset().mockResolvedValue({ blocker: null, appToQuit: null })
  vi.mocked(useSidebarEntries).mockReset().mockReturnValue(sidebarEntries)
  vi.mocked(planSessionMove).mockReset().mockResolvedValue({
    summary: 'Moves 1 file from Work to Personal',
    items: [],
    destinationNewer: false,
    desktop: 'add',
    blockers: [],
    appsToQuit: [],
    notes: [],
  })
})

/**
 * The row that shows `title`.
 */
function row(title: string): HTMLElement {
  const match = rows().find((item) => within(item).queryByText(title) !== null)
  if (match === undefined) {
    throw new Error(`No row shows ${title}`)
  }
  return match
}

describe('SessionsPanel', () => {
  it('offers to repair the sessions that need it at the top of the Active tab only', async () => {
    vi.mocked(listSessions).mockResolvedValue({
      sessions: [makeSession({ id: 'a', title: 'Plan the launch', kind: 'desktop', needsRepair: true }), ...mixed],
      repairCount: 1,
    })
    const { user } = await renderPanel()
    expect(screen.getByText(/1 session needs fixing/)).toBeInTheDocument()
    await user.click(screen.getByRole('tab', { name: /Archived/ }))
    expect(screen.queryByText(/needs fixing/)).toBeNull()
  })

  it('shows no repair offer when no session needs it', async () => {
    mockSessions(mixed)
    await renderPanel()
    expect(screen.queryByText(/repair/)).toBeNull()
  })

  it('lists the profile’s active sessions, most recently used first', async () => {
    mockSessions(mixed)
    await renderPanel()
    expect(listSessions).toHaveBeenCalledWith('p1')
    expectRows(['Refactor the parser', 'Plan the launch', 'Fix the build'])
  })

  it('narrows the rows as the search is typed', async () => {
    mockSessions(mixed)
    const { user } = await renderPanel()
    await user.type(screen.getByRole('searchbox'), 'parser')
    expectRows(['Refactor the parser'])
  })

  it('shows archived sessions on the Archived tab', async () => {
    mockSessions(mixed)
    const { user } = await renderPanel()
    await user.click(screen.getByRole('tab', { name: /Archived/ }))
    expectRows(['Old parser work'])
  })

  it('offers the kind filter only when the tab mixes both kinds', async () => {
    mockSessions(mixed)
    const { user } = await renderPanel()
    await user.click(screen.getByRole('radio', { name: 'Desktop' }))
    expectRows(['Plan the launch'])

    // The Archived tab holds only CLI sessions: the filter goes away, and the
    // Desktop choice made on the other tab no longer hides anything.
    await user.click(screen.getByRole('tab', { name: /Archived/ }))
    expect(screen.queryByRole('radiogroup')).toBeNull()
    expectRows(['Old parser work'])
  })

  it('reverses the order when the sort is toggled', async () => {
    mockSessions(mixed)
    const { user } = await renderPanel()
    await user.click(screen.getByRole('button', { name: /Last used/ }))
    expectRows(['Fix the build', 'Plan the launch', 'Refactor the parser'])
    await user.click(screen.getByRole('button', { name: /Last used/ }))
    expectRows(['Refactor the parser', 'Plan the launch', 'Fix the build'])
  })

  it('keeps the search across tabs and clears it from the no-match state', async () => {
    mockSessions(mixed)
    const { user } = await renderPanel()
    await user.type(screen.getByRole('searchbox'), 'launch')
    await user.click(screen.getByRole('tab', { name: /Archived/ }))
    expect(screen.queryByRole('list', { name: 'Sessions' })).toBeNull()

    await user.click(screen.getByRole('button', { name: 'Clear search' }))
    expect(screen.getByRole('searchbox')).toHaveValue('')
    expectRows(['Old parser work'])
  })

  it('finds an untitled session by its folder and acts on it', async () => {
    mockSessions([makeSession({ id: 'x', title: null, cwd: '/Users/ada/Developer/app' }), ...mixed])
    const { user } = await renderPanel()
    await user.type(screen.getByRole('searchbox'), 'Developer/app')
    expect(rows()).toHaveLength(1)
    const [untitled] = rows()

    await user.click(within(untitled).getByRole('button', { name: 'Archive' }))
    await screen.findByRole('dialog')
    expect(checkSessionAction).toHaveBeenCalledWith('p1', 'x', 'archive')
  })

  it('shows neither rows nor counts until the first listing lands', async () => {
    let land: (list: SessionList) => void = () => {}
    vi.mocked(listSessions).mockReturnValue(
      new Promise((resolve) => {
        land = resolve
      }),
    )
    renderWithQuery(<SessionsPanel profileId="p1" app="claude" />)

    expect(screen.getByRole('tab', { name: 'Active' })).toBeInTheDocument()
    expect(screen.queryByRole('list', { name: 'Sessions' })).toBeNull()
    expect(screen.queryByText('No sessions yet')).toBeNull()

    await act(async () => land({ sessions: mixed, repairCount: 0 }))
    expect(await screen.findByRole('tab', { name: 'Active 3' })).toBeInTheDocument()
    expectRows(['Refactor the parser', 'Plan the launch', 'Fix the build'])
  })

  it('says so when the profile has no sessions yet, on either tab', async () => {
    mockSessions([])
    const user = userEvent.setup()
    renderWithQuery(<SessionsPanel profileId="p1" app="claude" />)

    expect(await screen.findByText('No sessions yet')).toBeInTheDocument()
    expect(screen.queryByRole('list', { name: 'Sessions' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Clear search' })).toBeNull()

    await user.click(screen.getByRole('tab', { name: /Archived/ }))
    expect(screen.getByText('No archived sessions')).toBeInTheDocument()
  })

  it('retries a failed listing once, then shows the error inline and refetches on Retry', async () => {
    const failure = { kind: 'Io', message: 'codex app-server exited' }
    vi.mocked(listSessions)
      .mockRejectedValueOnce(failure)
      .mockRejectedValueOnce(failure)
      .mockResolvedValueOnce({ sessions: mixed, repairCount: 0 })
    const user = userEvent.setup()
    renderWithQuery(<SessionsPanel profileId="default:codex" app="codex" />, { client: makeRetryingClient() })

    expect(await screen.findByRole('alert')).toHaveTextContent('codex app-server exited')
    expect(listSessions).toHaveBeenCalledTimes(2)
    await user.click(screen.getByRole('button', { name: 'Retry' }))
    await screen.findByRole('list', { name: 'Sessions' })
    expect(listSessions).toHaveBeenCalledTimes(3)
    expect(listSessions).toHaveBeenLastCalledWith('default:codex')
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('keeps the last list when a refresh fails, and refreshes again on Retry', async () => {
    const failure = { kind: 'Io', message: 'codex app-server exited' }
    mockSessions(mixed)
    const user = userEvent.setup()
    const { client } = renderWithQuery(<SessionsPanel profileId="p1" app="claude" />, { client: makeRetryingClient() })
    await screen.findByRole('list', { name: 'Sessions' })
    vi.mocked(listSessions).mockRejectedValueOnce(failure).mockRejectedValueOnce(failure)
    await act(() => client.refetchQueries())

    const retry = await screen.findByRole('button', { name: 'Retry' })
    expectRows(['Refactor the parser', 'Plan the launch', 'Fix the build'])
    expect(screen.queryByRole('alert')).toBeNull()
    await user.click(retry)

    await waitFor(() => expect(screen.queryByRole('button', { name: 'Retry' })).toBeNull())
    expect(listSessions).toHaveBeenCalledTimes(4)
    expectRows(['Refactor the parser', 'Plan the launch', 'Fix the build'])
  })

  it('explains calmly, without a Retry, that a Codex profile needs the CLI', async () => {
    vi.mocked(listSessions).mockRejectedValue({
      kind: 'NotInstalled',
      message: "Install the Codex CLI to see this profile's sessions",
    })
    renderWithQuery(<SessionsPanel profileId="default:codex" app="codex" />, { client: makeRetryingClient() })

    expect(await screen.findAllByText("Install the Codex CLI to see this profile's sessions")).not.toHaveLength(0)
    expect(listSessions).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('alert')).toBeNull()
    expect(screen.queryByRole('button', { name: 'Retry' })).toBeNull()
  })

  it('archives a session from its row once the user confirms', async () => {
    mockSessions(mixed)
    const { user } = await renderPanel()
    await user.click(within(row('Plan the launch')).getByRole('button', { name: 'Archive' }))
    const dialog = await screen.findByRole('dialog', { name: /Plan the launch/ })
    expect(checkSessionAction).toHaveBeenCalledWith('p1', 'b', 'archive')
    await waitFor(() => expect(within(dialog).getByRole('button', { name: /^Archive/ })).toBeEnabled())
    await user.click(within(dialog).getByRole('button', { name: /^Archive/ }))
    expect(archiveSession).toHaveBeenCalledWith('p1', 'b', false)
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  })

  it('archives a Codex session from its row, the same as a Claude one', async () => {
    mockSessions([makeSession({ id: 'c1', title: 'Fix the flaky test' })])
    const user = userEvent.setup()
    renderWithQuery(
      <ToastProvider>
        <SessionsPanel profileId="default:codex" app="codex" />
      </ToastProvider>,
    )
    await screen.findByRole('list', { name: 'Sessions' })

    await user.click(within(row('Fix the flaky test')).getByRole('button', { name: 'Archive' }))

    await screen.findByRole('dialog', { name: /Fix the flaky test/ })
    expect(checkSessionAction).toHaveBeenCalledWith('default:codex', 'c1', 'archive')
  })

  it('offers restoring on the Archived tab', async () => {
    mockSessions(mixed)
    const { user } = await renderPanel()
    await user.click(screen.getByRole('tab', { name: /Archived/ }))
    await user.click(within(row('Old parser work')).getByRole('button', { name: 'Restore' }))
    await screen.findByRole('dialog', { name: /Old parser work/ })
    expect(checkSessionAction).toHaveBeenCalledWith('p1', 'd', 'restore')
  })

  it('holds archiving back while a terminal has the session open', async () => {
    mockSessions([makeSession({ id: 'busy', title: 'Busy', state: 'openInTerminal' })])
    const { user } = await renderPanel()
    const archive = within(row('Busy')).getByRole('button', { name: 'Archive' })
    expect(archive).toHaveAttribute('aria-disabled', 'true')
    await user.click(archive)
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('says why archiving is held back in the words of the session’s app', async () => {
    mockSessions([makeSession({ id: 'busy', title: 'Busy', state: 'openInTerminal' })])
    renderWithQuery(
      <ToastProvider>
        <SessionsPanel profileId="default:codex" app="codex" />
      </ToastProvider>,
    )
    await screen.findByRole('list', { name: 'Sessions' })

    const archive = within(row('Busy')).getByRole('button', { name: 'Archive' })
    expect(archive).toHaveAccessibleDescription('Codex has it open — close it first')
  })

  it('says what has an open session open in the words of the session’s app', async () => {
    mockSessions([makeSession({ id: 'busy', title: 'Busy', state: 'openInTerminal' })])
    renderWithQuery(
      <ToastProvider>
        <SessionsPanel profileId="default:codex" app="codex" />
      </ToastProvider>,
    )
    await screen.findByRole('list', { name: 'Sessions' })

    expect(within(row('Busy')).getByTitle('Codex has it open')).toHaveTextContent('Open')
  })

  it('moves a session to another profile of the app picked from its row', async () => {
    mockSessions(mixed)
    const { user } = await renderPanel()
    await user.click(within(row('Plan the launch')).getByRole('button', { name: 'Move' }))
    const targets = (await screen.findAllByRole('menuitem')).map((item) => item.textContent)
    expect(targets).toEqual(['Claude', 'Personal'])
    await user.click(screen.getByRole('menuitem', { name: 'Personal' }))
    await screen.findByRole('dialog', { name: /Plan the launch/ })
    expect(planSessionMove).toHaveBeenCalledWith('p1', 'b', 'p2')
  })

  it('moves a Codex session to another Codex profile picked from its row', async () => {
    mockSessions([makeSession({ id: 'c1', title: 'Fix the flaky test' })])
    vi.mocked(planSessionMove).mockResolvedValue({
      summary: 'Moves 1 file from Codex to Chat',
      items: [{ path: 'sessions/2026/09/01/rollout-c1.jsonl', action: 'copy' }],
      destinationNewer: false,
      desktop: 'noDesktop',
      blockers: [],
      appsToQuit: [],
      notes: [],
    })
    vi.mocked(moveSession).mockReset().mockResolvedValue({ memoryConflicts: [] })
    const user = userEvent.setup()
    renderWithQuery(
      <ToastProvider>
        <SessionsPanel profileId="default:codex" app="codex" />
      </ToastProvider>,
    )
    await screen.findByRole('list', { name: 'Sessions' })

    await user.click(within(row('Fix the flaky test')).getByRole('button', { name: 'Move' }))
    const targets = (await screen.findAllByRole('menuitem')).map((item) => item.textContent)
    expect(targets).toEqual(['Chat'])
    await user.click(screen.getByRole('menuitem', { name: 'Chat' }))
    const dialog = await screen.findByRole('dialog', { name: /Fix the flaky test/ })
    expect(planSessionMove).toHaveBeenCalledWith('default:codex', 'c1', 'p3')
    await user.click(await within(dialog).findByRole('button', { name: /^Move/ }))

    expect(moveSession).toHaveBeenCalledWith('default:codex', 'c1', 'p3', false, false)
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  })

  it('holds moving back with the reason the session can’t move', async () => {
    mockSessions([makeSession({ id: 'gone', title: 'Gone', unmovableReason: 'Transcript deleted' })])
    const { user } = await renderPanel()
    const move = within(row('Gone')).getByRole('button', { name: 'Move' })
    expect(move).toHaveAttribute('aria-disabled', 'true')
    await user.click(move)
    expect(screen.queryByRole('menuitem')).toBeNull()
  })

  it('offers no move when the app has no other profile', async () => {
    vi.mocked(useSidebarEntries).mockReturnValue([managedEntry('p1', 'Work', 'claude')])
    mockSessions(mixed)
    await renderPanel()
    expect(within(row('Plan the launch')).queryByRole('button', { name: 'Move' })).toBeNull()
  })
})
