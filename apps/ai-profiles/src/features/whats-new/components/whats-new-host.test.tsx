import type { AppMetadata } from '@/lib/types'

import { screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { getAppMetadata } from '@/lib/commands'
import { renderWithQuery } from '@/test/render-with-query'

import { lastSeenVersionKey } from '../api/last-seen-version'
import { WhatsNewHost } from './whats-new-host'

vi.mock('@/lib/commands', () => ({ getAppMetadata: vi.fn() }))

const showSpy = vi.fn()

vi.mock('@/design', async () => {
  const actual = await vi.importActual<typeof import('@/design')>('@/design')
  return {
    ...actual,
    useToast: () => ({
      show: showSpy,
      success: () => '',
      error: () => '',
      info: () => '',
      dismiss: () => undefined,
    }),
  }
})

function makeMetadata(version: string): AppMetadata {
  return { name: 'ai-profiles', version, description: '', authors: [], repository: null, homepage: null, license: null }
}

beforeEach(() => {
  window.localStorage.clear()
  showSpy.mockReset()
  vi.mocked(getAppMetadata).mockReset()
  vi.mocked(getAppMetadata).mockResolvedValue(makeMetadata('1.1.0'))
})

describe('WhatsNewHost', () => {
  it('toasts once after an upgrade and the toast action opens the dialog', async () => {
    window.localStorage.setItem(lastSeenVersionKey, '1.0.2')
    const onOpen = vi.fn()

    const { rerender } = renderWithQuery(<WhatsNewHost open={false} onOpen={onOpen} onClose={vi.fn()} />)

    await waitFor(() => expect(showSpy).toHaveBeenCalledTimes(1))
    showSpy.mock.calls[0][0].action.onClick()
    expect(onOpen).toHaveBeenCalledTimes(1)

    // A re-render with a new `onOpen` (and, via the mock, a new toast object) re-runs the effect;
    // the ref guard must keep it from toasting the same version again.
    rerender(<WhatsNewHost open={false} onOpen={() => undefined} onClose={vi.fn()} />)
    expect(showSpy).toHaveBeenCalledTimes(1)
  })

  it('does not toast when the version is unchanged', async () => {
    window.localStorage.setItem(lastSeenVersionKey, '1.1.0')

    renderWithQuery(<WhatsNewHost open onOpen={vi.fn()} onClose={vi.fn()} />)

    // The dialog only renders once the app metadata has resolved and the host's effects have run.
    await screen.findByRole('dialog')
    expect(showSpy).not.toHaveBeenCalled()
  })

  it('does not toast on a fresh install, and records the version', async () => {
    renderWithQuery(<WhatsNewHost open onOpen={vi.fn()} onClose={vi.fn()} />)

    await screen.findByRole('dialog')
    await waitFor(() => expect(window.localStorage.getItem(lastSeenVersionKey)).toBe('1.1.0'))
    expect(showSpy).not.toHaveBeenCalled()
  })
})
