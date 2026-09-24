import type { ReactNode } from 'react'
import type { QueryClient } from '@tanstack/react-query'
import type { ActionCheck, Session } from '@/lib/types'

import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'
import { archiveSession, checkSessionAction, restoreSession } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'
import { makeRetryingClient, renderWithQuery } from '@/test/render-with-query'

import { makeSession } from '../test/make-session'
import { ConfirmSessionActionDialog } from './confirm-session-action-dialog'

vi.mock('@/lib/commands', () => ({
  archiveSession: vi.fn(),
  checkSessionAction: vi.fn(),
  restoreSession: vi.fn(),
}))

/**
 * The desktop session each case acts on, overridden per case.
 */
function loginBugSession(overrides: Partial<Session> = {}): Session {
  return makeSession({ kind: 'desktop', title: 'Fix the login bug', ...overrides })
}

/**
 * Makes the next check resolve with `check`.
 */
function mockCheck(check: Partial<ActionCheck>) {
  vi.mocked(checkSessionAction).mockResolvedValue({ blocker: null, appToQuit: null, ...check })
}

/**
 * Hosts the toasts the dialog raises.
 */
function withToasts(ui: ReactNode) {
  return <ToastProvider>{ui}</ToastProvider>
}

/**
 * Renders the dialog for `session` and waits for its check to land.
 */
async function renderDialog(session: Session, action: 'archive' | 'restore' = 'archive', client?: QueryClient) {
  const onClose = vi.fn()
  const user = userEvent.setup()
  const result = renderWithQuery(
    withToasts(<ConfirmSessionActionDialog profileId="p1" session={session} action={action} onClose={onClose} />),
    { client },
  )
  await waitFor(() => expect(checkSessionAction).toHaveBeenCalledWith('p1', session.id, action))
  return { ...result, onClose, user }
}

beforeEach(() => {
  vi.mocked(archiveSession).mockReset().mockResolvedValue(undefined)
  vi.mocked(restoreSession).mockReset().mockResolvedValue(undefined)
  vi.mocked(checkSessionAction).mockReset()
})

describe('ConfirmSessionActionDialog', () => {
  it('offers no way to go ahead while something only the user can clear stands in the way', async () => {
    mockCheck({ blocker: 'Close it in the terminal first' })
    const { user, onClose } = await renderDialog(loginBugSession())
    expect(await screen.findByText('Close it in the terminal first')).toBeInTheDocument()
    expect(screen.getAllByRole('button').filter((button) => !button.hasAttribute('disabled'))).toHaveLength(1)
    await user.keyboard('{Enter}')
    expect(archiveSession).not.toHaveBeenCalled()
    await user.click(screen.getByRole('button', { name: /Cancel/ }))
    expect(onClose).toHaveBeenCalledOnce()
  })

  it('quits the desktop app in the way when the user confirms', async () => {
    mockCheck({ appToQuit: { homeId: 'p1', label: 'Claude (Work)' } })
    const { user } = await renderDialog(loginBugSession())
    await user.click(await screen.findByRole('button', { name: /Claude \(Work\)/ }))
    expect(archiveSession).toHaveBeenCalledWith('p1', 's1', true)
  })

  it('goes ahead without quitting anything when nothing is in the way', async () => {
    mockCheck({})
    const { user } = await renderDialog(loginBugSession({ archived: true }), 'restore')
    await waitFor(() => expect(screen.getByRole('button', { name: /^Restore/ })).toBeEnabled())
    await user.click(screen.getByRole('button', { name: /^Restore/ }))
    expect(restoreSession).toHaveBeenCalledWith('p1', 's1', false)
  })

  it('closes and refreshes the session lists once the action is done', async () => {
    mockCheck({})
    const { user, onClose, client } = await renderDialog(loginBugSession())
    const invalidate = vi.spyOn(client, 'invalidateQueries')
    await waitFor(() => expect(screen.getByRole('button', { name: /^Archive/ })).toBeEnabled())
    await user.click(screen.getByRole('button', { name: /^Archive/ }))
    await waitFor(() => expect(onClose).toHaveBeenCalledOnce())
    expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.sessions.all })
  })

  it('stays open and says why when the action fails', async () => {
    mockCheck({ appToQuit: { homeId: 'p1', label: 'Claude (Work)' } })
    vi.mocked(archiveSession).mockRejectedValue({ kind: 'Validation', message: 'Claude (Work) didn’t quit' })
    const { user, onClose } = await renderDialog(loginBugSession())
    await user.click(await screen.findByRole('button', { name: /Claude \(Work\)/ }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Claude (Work) didn’t quit')
    expect(onClose).not.toHaveBeenCalled()
  })

  it('explains calmly that the Codex CLI is needed, with no way to go ahead', async () => {
    vi.mocked(checkSessionAction).mockRejectedValue({
      kind: 'NotInstalled',
      message: 'Install the Codex CLI to archive or restore this session',
    })
    await renderDialog(loginBugSession(), 'archive', makeRetryingClient())
    expect(await screen.findByText('Install the Codex CLI to archive or restore this session')).toBeInTheDocument()
    expect(checkSessionAction).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('alert')).toBeNull()
    expect(screen.getByRole('button', { name: /^Archive/ })).toBeDisabled()
  })
})
