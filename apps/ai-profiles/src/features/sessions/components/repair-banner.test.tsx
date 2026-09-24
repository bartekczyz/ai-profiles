import type { ReactNode } from 'react'
import type { ActionCheck, RepairReport } from '@/lib/types'

import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'
import { checkSessionRepair, repairSessions } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'
import { renderWithQuery } from '@/test/render-with-query'

import { RepairBanner } from './repair-banner'

vi.mock('@/lib/commands', () => ({
  checkSessionRepair: vi.fn(),
  repairSessions: vi.fn(),
}))

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
 * Renders the banner for `repairCount` sessions of profile `p1`.
 */
function renderBanner(repairCount: number) {
  const user = userEvent.setup()
  const result = renderWithQuery(
    withToasts(<RepairBanner profileId="p1" profileLabel="Personal" repairCount={repairCount} />),
  )
  return { ...result, user }
}

/**
 * Opens the confirm dialog from the banner and waits for its check to land.
 */
async function openDialog(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole('button', { name: /Repair/ }))
  await waitFor(() => expect(checkSessionRepair).toHaveBeenCalledWith('p1'))
  return screen.findByRole('dialog')
}

beforeEach(() => {
  vi.mocked(checkSessionRepair).mockReset()
  vi.mocked(repairSessions).mockReset()
})

describe('RepairBanner', () => {
  it('shows nothing when no session needs repair', () => {
    renderBanner(0)
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
    expect(checkSessionRepair).not.toHaveBeenCalled()
  })

  it('repairs once confirmed, refreshes the session lists and says what it did', async () => {
    mockCheck({})
    mockRepair({ repaired: 2, skipped: [{ id: 's3', reason: 'Close it in the terminal first' }] })
    const { user, client } = renderBanner(3)
    expect(screen.getByText(/3 sessions need repair/)).toBeInTheDocument()
    const invalidate = vi.spyOn(client, 'invalidateQueries')
    const dialog = await openDialog(user)
    expect(dialog).toHaveTextContent('Personal')
    expect(repairSessions).not.toHaveBeenCalled()
    await waitFor(() => expect(screen.getByRole('button', { name: /^Repair/ })).toBeEnabled())
    await user.click(screen.getByRole('button', { name: /^Repair/ }))
    expect(repairSessions).toHaveBeenCalledWith('p1', false)
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.sessions.all })
    const summary = await screen.findByText(/2 repaired/)
    expect(summary).toHaveTextContent(/1 skipped/)
    expect(summary).toHaveTextContent(/Close it in the terminal first/)
  })

  it('quits the profile’s desktop app first when it runs', async () => {
    mockCheck({ appToQuit: { homeId: 'p1', label: 'Claude (Personal)' } })
    mockRepair({ repaired: 3 })
    const { user } = renderBanner(3)
    await openDialog(user)
    await user.click(await screen.findByRole('button', { name: /Claude \(Personal\)/ }))
    expect(repairSessions).toHaveBeenCalledWith('p1', true)
  })

  it('repairs nothing when the user cancels', async () => {
    mockCheck({})
    const { user } = renderBanner(1)
    await openDialog(user)
    await user.click(screen.getByRole('button', { name: /Cancel/ }))
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    expect(repairSessions).not.toHaveBeenCalled()
  })

  it('stays open and says why when the repair fails', async () => {
    mockCheck({ appToQuit: { homeId: 'p1', label: 'Claude (Personal)' } })
    vi.mocked(repairSessions).mockRejectedValue({ kind: 'Validation', message: 'Claude (Personal) didn’t quit' })
    const { user } = renderBanner(2)
    await openDialog(user)
    await user.click(await screen.findByRole('button', { name: /Claude \(Personal\)/ }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Claude (Personal) didn’t quit')
    expect(screen.getByRole('dialog')).toBeInTheDocument()
  })
})
