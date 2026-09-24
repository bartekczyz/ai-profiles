import type { QueryClient } from '@tanstack/react-query'
import type { MovePlan, Session } from '@/lib/types'

import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'
import { moveSession, planSessionMove } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'
import { makeRetryingClient, renderWithQuery } from '@/test/render-with-query'

import { makeSession } from '../test/make-session'
import { MoveSessionDialog } from './move-session-dialog'

vi.mock('@/lib/commands', () => ({
  moveSession: vi.fn(),
  planSessionMove: vi.fn(),
}))

/**
 * The desktop session each case acts on, overridden per case.
 */
function loginBugSession(overrides: Partial<Session> = {}): Session {
  return makeSession({ kind: 'desktop', title: 'Fix the login bug', ...overrides })
}

/**
 * Makes the plan resolve with a move of one file, overridden per case.
 */
function mockPlan(overrides: Partial<MovePlan> = {}) {
  vi.mocked(planSessionMove).mockResolvedValue({
    summary: 'Moves 1 file from Work to Personal',
    items: [{ path: 'projects/-work-app/s1.jsonl', action: 'copy' }],
    destinationNewer: false,
    desktop: 'add',
    blockers: [],
    appsToQuit: [],
    notes: [],
    ...overrides,
  })
}

/**
 * Renders the dialog moving the session to Personal and waits for its plan.
 */
async function renderDialog(client?: QueryClient) {
  const onClose = vi.fn()
  const user = userEvent.setup()
  const result = renderWithQuery(
    <ToastProvider>
      <MoveSessionDialog
        profileId="work"
        session={loginBugSession()}
        destination={{ id: 'personal', label: 'Personal' }}
        onClose={onClose}
      />
    </ToastProvider>,
    { client },
  )
  await waitFor(() => expect(planSessionMove).toHaveBeenCalledWith('work', 's1', 'personal'))
  return { ...result, onClose, user }
}

beforeEach(() => {
  vi.mocked(planSessionMove).mockReset()
  vi.mocked(moveSession).mockReset().mockResolvedValue({ memoryConflicts: [] })
})

describe('MoveSessionDialog', () => {
  it('moves only once the user agrees to replace a newer copy at the destination', async () => {
    mockPlan({ destinationNewer: true })
    const { user } = await renderDialog()
    const move = await screen.findByRole('button', { name: /^Move/ })
    expect(move).toBeDisabled()
    await user.click(screen.getByRole('checkbox', { name: /Replace newer copy/ }))
    expect(move).toBeEnabled()
    await user.click(move)
    expect(moveSession).toHaveBeenCalledWith('work', 's1', 'personal', true, false)
  })

  it('offers no replace choice when the destination has nothing newer', async () => {
    mockPlan()
    const { user } = await renderDialog()
    await waitFor(() => expect(screen.getByRole('button', { name: /^Move/ })).toBeEnabled())
    expect(screen.queryByRole('checkbox')).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /^Move/ }))
    expect(moveSession).toHaveBeenCalledWith('work', 's1', 'personal', false, false)
  })

  it('names the one desktop app in the way on the button and quits it', async () => {
    mockPlan({ appsToQuit: [{ homeId: 'work', label: 'Claude (Work)' }] })
    const { user } = await renderDialog()
    await user.click(await screen.findByRole('button', { name: /Quit Claude \(Work\) and move/ }))
    expect(moveSession).toHaveBeenCalledWith('work', 's1', 'personal', false, true)
  })

  it('quits both desktop apps in the way', async () => {
    mockPlan({
      appsToQuit: [
        { homeId: 'work', label: 'Claude (Work)' },
        { homeId: 'personal', label: 'Claude (Personal)' },
      ],
    })
    const { user } = await renderDialog()
    await user.click(await screen.findByRole('button', { name: /Quit both and move/ }))
    expect(moveSession).toHaveBeenCalledWith('work', 's1', 'personal', false, true)
  })

  it('offers no way to go ahead while something only the user can clear stands in the way', async () => {
    mockPlan({ blockers: ['Close it in the terminal first'] })
    const { user } = await renderDialog()
    expect(await screen.findByText('Close it in the terminal first')).toBeInTheDocument()
    await user.keyboard('{Enter}')
    expect(moveSession).not.toHaveBeenCalled()
  })

  it('names every obstacle when more than one stands in the way', async () => {
    mockPlan({ blockers: ['Close it in the terminal first', 'The destination is being updated'] })
    await renderDialog()
    const reasons = await screen.findByText(/Close it in the terminal first/)
    expect(reasons).toHaveTextContent('The destination is being updated')
  })

  it('closes, refreshes the session lists and says where the session went once moved', async () => {
    mockPlan()
    vi.mocked(moveSession).mockResolvedValue({ memoryConflicts: ['deploy.md'] })
    const { user, onClose, client } = await renderDialog()
    const invalidate = vi.spyOn(client, 'invalidateQueries')
    await waitFor(() => expect(screen.getByRole('button', { name: /^Move/ })).toBeEnabled())
    await user.click(screen.getByRole('button', { name: /^Move/ }))
    await waitFor(() => expect(onClose).toHaveBeenCalledOnce())
    expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.sessions.all })
    expect((await screen.findAllByText('Moved to Personal')).length).toBeGreaterThan(0)
    expect(screen.getAllByText(/deploy\.md/).length).toBeGreaterThan(0)
  })

  it('stays open, says why and plans again when the move fails', async () => {
    mockPlan()
    vi.mocked(moveSession).mockRejectedValue({ kind: 'Validation', message: 'Claude (Personal) didn’t quit' })
    const { user, onClose } = await renderDialog()
    await waitFor(() => expect(screen.getByRole('button', { name: /^Move/ })).toBeEnabled())
    await user.click(screen.getByRole('button', { name: /^Move/ }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Claude (Personal) didn’t quit')
    expect(onClose).not.toHaveBeenCalled()
    await waitFor(() => expect(planSessionMove).toHaveBeenCalledTimes(2))
  })

  it('explains calmly that the Codex CLI is needed, with no way to go ahead', async () => {
    vi.mocked(planSessionMove).mockRejectedValue({
      kind: 'NotInstalled',
      message: 'Install the Codex CLI to move this session',
    })
    await renderDialog(makeRetryingClient())
    expect(await screen.findByText('Install the Codex CLI to move this session')).toBeInTheDocument()
    expect(planSessionMove).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('alert')).toBeNull()
    expect(screen.getByRole('button', { name: /^Move/ })).toBeDisabled()
  })
})
