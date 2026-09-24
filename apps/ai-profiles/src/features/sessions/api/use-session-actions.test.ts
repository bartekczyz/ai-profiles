import type { MovePlan } from '@/lib/types'

import { focusManager } from '@tanstack/react-query'
import { act, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { planSessionMove } from '@/lib/commands'
import { renderHookWithQuery } from '@/test/render-with-query'

import { useSessionMovePlan } from './use-session-actions'

vi.mock('@/lib/commands', () => ({ planSessionMove: vi.fn() }))

afterEach(() => {
  focusManager.setFocused(undefined)
  vi.mocked(planSessionMove).mockReset()
})

/**
 * A plan of a move of one file, overridden per case.
 */
function makePlan(overrides: Partial<MovePlan> = {}): MovePlan {
  return {
    summary: 'Moves 1 file from Work to Personal',
    items: [{ path: 'projects/-work-app/s1.jsonl', action: 'copy' }],
    destinationNewer: false,
    desktop: 'noDesktop',
    blockers: [],
    appsToQuit: [],
    notes: [],
    ...overrides,
  }
}

/**
 * Plans a move with `plan`, then brings the window back into focus. Returns
 * how many times the move was planned again.
 */
async function planThenRefocus(plan: MovePlan): Promise<number> {
  vi.mocked(planSessionMove).mockResolvedValue(plan)
  const { result } = renderHookWithQuery(() => useSessionMovePlan('p1', 's1', 'p2'))
  await waitFor(() => expect(result.current.isSuccess).toBe(true))
  const planned = vi.mocked(planSessionMove).mock.calls.length

  act(() => {
    focusManager.setFocused(false)
    focusManager.setFocused(true)
  })

  // A refetch the focus started is under way by now; wait for it to settle.
  await waitFor(() => expect(result.current.isFetching).toBe(false))
  return vi.mocked(planSessionMove).mock.calls.length - planned
}

describe('useSessionMovePlan', () => {
  it('does not plan again on focus when nothing stands in the way', async () => {
    expect(await planThenRefocus(makePlan())).toBe(0)
  })

  it('plans again on focus while something the user can clear stands in the way', async () => {
    expect(await planThenRefocus(makePlan({ blockers: ['Close it in the terminal first'] }))).toBe(1)
  })

  it('plans again on focus while a desktop app has to quit first', async () => {
    const appsToQuit = [{ homeId: 'p2', label: 'Claude (Personal)' }]

    expect(await planThenRefocus(makePlan({ appsToQuit }))).toBe(1)
  })
})
