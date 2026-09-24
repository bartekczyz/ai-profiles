import type { ReactNode } from 'react'
import type { ActionCheck, AppState, RepairReport } from '@/lib/types'

import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'
import { checkSessionRepair, loadAppState, repairSessions, updateAppState } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'
import { renderWithQuery } from '@/test/render-with-query'

import { RepairBanner } from './repair-banner'

vi.mock('@/lib/commands', () => ({
  checkSessionRepair: vi.fn(),
  repairSessions: vi.fn(),
  loadAppState: vi.fn(),
  updateAppState: vi.fn(),
}))

/**
 * The app state, with the repair offers dismissed so far.
 */
function appState(dismissedRepairSessions: AppState['dismissedRepairSessions'] = {}): AppState {
  return {
    welcomeShown: true,
    migrationDismissedAt: null,
    pathBannerDismissedAt: null,
    themeMode: 'system',
    selectedEntryId: null,
    dockIconAcknowledgedAt: null,
    defaultProfileNames: {},
    dismissedRepairSessions,
  }
}

/**
 * Makes the next check resolve with `check`.
 */
function mockCheck(check: Partial<ActionCheck>) {
  vi.mocked(checkSessionRepair).mockResolvedValue({ blocker: null, appToQuit: null, ...check })
}

/**
 * Makes the next repair resolve with `report`.
 */
function mockRepair(report: Partial<RepairReport>) {
  vi.mocked(repairSessions).mockResolvedValue({
    repaired: 0,
    skipped: [],
    memoryConflicts: [],
    warnings: [],
    ...report,
  })
}

/**
 * Hosts the toasts the banner raises.
 */
function withToasts(ui: ReactNode) {
  return <ToastProvider>{ui}</ToastProvider>
}

/**
 * Renders the banner of profile `p1`, whose sessions `ids` need repair.
 */
function renderBanner(ids: Array<string>) {
  const user = userEvent.setup()
  const result = renderWithQuery(
    withToasts(<RepairBanner profileId="p1" profileLabel="Personal" repairSessionIds={ids} />),
    { suspenseFallback: <p>Loading</p> },
  )
  return { ...result, user }
}

/**
 * `count` session ids.
 */
function ids(count: number) {
  return Array.from({ length: count }, (_, index) => `s${index}`)
}

/**
 * Opens the confirm dialog from the banner and waits for its check to land.
 */
async function openDialog(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole('button', { name: /Repair/ }))
  await waitFor(() => expect(checkSessionRepair).toHaveBeenCalledWith('p1'))
  return screen.findByRole('dialog')
}

beforeEach(() => {
  vi.mocked(checkSessionRepair).mockReset()
  vi.mocked(repairSessions).mockReset()
  vi.mocked(loadAppState).mockResolvedValue(appState())
  vi.mocked(updateAppState).mockImplementation(async (patch) =>
    appState({ p1: patch.dismissedRepair?.sessionIds ?? [] }),
  )
})

describe('RepairBanner', () => {
  it('shows nothing when no session needs repair', () => {
    renderBanner([])
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
    expect(checkSessionRepair).not.toHaveBeenCalled()
  })

  it('repairs once confirmed, refreshes the session lists and says what it did', async () => {
    mockCheck({})
    mockRepair({ repaired: 2, skipped: [{ id: 's3', reason: 'Close it in the terminal first' }] })
    const { user, client } = renderBanner(ids(3))
    expect(await screen.findByText(/3 sessions need fixing/)).toBeInTheDocument()
    const invalidate = vi.spyOn(client, 'invalidateQueries')
    const dialog = await openDialog(user)
    expect(dialog).toHaveTextContent('Personal')
    expect(repairSessions).not.toHaveBeenCalled()
    await waitFor(() => expect(screen.getByRole('button', { name: /^Repair/ })).toBeEnabled())
    await user.click(screen.getByRole('button', { name: /^Repair/ }))
    expect(repairSessions).toHaveBeenCalledWith('p1', false)
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.sessions.all })
    // The toast's text can show twice while it is announced to screen readers.
    const [summary] = await screen.findAllByText(/2 repaired/)
    expect(summary).toHaveTextContent(/1 skipped/)
    expect(summary).toHaveTextContent(/Close it in the terminal first/)
  })

  it('quits the profile’s desktop app first when it runs', async () => {
    mockCheck({ appToQuit: { homeId: 'p1', label: 'Claude (Personal)' } })
    mockRepair({ repaired: 3 })
    const { user } = renderBanner(ids(3))
    await openDialog(user)
    await user.click(await screen.findByRole('button', { name: /Claude \(Personal\)/ }))
    expect(repairSessions).toHaveBeenCalledWith('p1', true)
  })

  it('repairs nothing when the user cancels', async () => {
    mockCheck({})
    const { user } = renderBanner(ids(1))
    await openDialog(user)
    await user.click(screen.getByRole('button', { name: /Cancel/ }))
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    expect(repairSessions).not.toHaveBeenCalled()
  })

  it('stays open and says why when the repair fails', async () => {
    mockCheck({ appToQuit: { homeId: 'p1', label: 'Claude (Personal)' } })
    vi.mocked(repairSessions).mockRejectedValue({ kind: 'Validation', message: 'Claude (Personal) didn’t quit' })
    const { user } = renderBanner(ids(2))
    await openDialog(user)
    await user.click(await screen.findByRole('button', { name: /Claude \(Personal\)/ }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Claude (Personal) didn’t quit')
    expect(screen.getByRole('dialog')).toBeInTheDocument()
  })
  it('puts the offer away for the sessions that need repair now', async () => {
    const { user } = renderBanner(ids(2))
    await user.click(await screen.findByRole('button', { name: 'Not now' }))
    expect(vi.mocked(updateAppState).mock.calls[0]?.[0]).toEqual({
      dismissedRepair: { profileId: 'p1', sessionIds: ['s0', 's1'] },
    })
    await waitFor(() => expect(screen.queryByText(/need fixing/)).not.toBeInTheDocument())
  })

  it('stays away while only dismissed sessions need repair', async () => {
    vi.mocked(loadAppState).mockResolvedValue(appState({ p1: ['s0', 's1'] }))
    renderBanner(ids(1))
    await waitFor(() => expect(screen.queryByText('Loading')).not.toBeInTheDocument())
    expect(screen.queryByText(/needs fixing/)).not.toBeInTheDocument()
  })

  it('comes back when another session needs repair', async () => {
    vi.mocked(loadAppState).mockResolvedValue(appState({ p1: ['s0'] }))
    renderBanner(ids(2))
    expect(await screen.findByText(/2 sessions need fixing/)).toBeInTheDocument()
  })
})
