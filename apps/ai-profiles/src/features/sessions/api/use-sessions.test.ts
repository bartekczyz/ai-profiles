import { focusManager } from '@tanstack/react-query'
import { act, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { listSessions } from '@/lib/commands'
import { renderHookWithQuery } from '@/test/render-with-query'

import { useSessions } from './use-sessions'

vi.mock('@/lib/commands', () => ({ listSessions: vi.fn(async () => ({ sessions: [], repairCount: 0 })) }))

afterEach(() => {
  focusManager.setFocused(undefined)
})

describe('useSessions', () => {
  it('lists again whenever the window regains focus, even while the data is fresh', async () => {
    // The test client keeps data fresh forever, so only an unconditional
    // focus refetch can call the command a second time.
    const { result } = renderHookWithQuery(() => useSessions('p1'))
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(listSessions).toHaveBeenCalledTimes(1)

    act(() => {
      focusManager.setFocused(false)
      focusManager.setFocused(true)
    })

    await waitFor(() => expect(listSessions).toHaveBeenCalledTimes(2))
    expect(listSessions).toHaveBeenLastCalledWith('p1')
  })
})
