import type { SidebarEntry } from '@/lib/types'

import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { useSidebarSelection } from './use-sidebar-selection'

const appStateFake = {
  state: { selectedEntryId: null as string | null },
  update: vi.fn(async (patch: { selectedEntryId?: string | null; clearSelectedEntryId?: boolean }) => {
    if (patch.clearSelectedEntryId) {
      appStateFake.state.selectedEntryId = null
    } else if (patch.selectedEntryId !== undefined) {
      appStateFake.state.selectedEntryId = patch.selectedEntryId
    }
  }),
}

vi.mock('@/lib/app-state/use-app-state', () => ({
  useAppState: () => appStateFake,
}))

function managed(id: string): SidebarEntry {
  return {
    kind: 'managed',
    profile: {
      id,
      app: 'claude',
      name: id,
      slug: id,
      color: '#000000',
      createdAt: '2026-05-20T00:00:00Z',
      lastUsedAt: null,
      surfaces: { gui: true, cli: true },
    },
  }
}

function defaultClaudeEntry(): SidebarEntry {
  return {
    kind: 'default',
    entry: {
      id: 'default:claude',
      app: 'claude',
      name: 'Default',
      surfaces: { gui: true, cli: true },
    },
  }
}

beforeEach(() => {
  appStateFake.state.selectedEntryId = null
  appStateFake.update.mockClear()
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('useSidebarSelection', () => {
  it('restores the persisted id when it matches an entry', () => {
    appStateFake.state.selectedEntryId = 'managed-2'
    const { result } = renderHook(() =>
      useSidebarSelection([defaultClaudeEntry(), managed('managed-1'), managed('managed-2')]),
    )
    expect(result.current.selectedId).toBe('managed-2')
  })

  it('falls back to entries[0] when the persisted id is absent from the list', () => {
    appStateFake.state.selectedEntryId = 'managed-gone'
    const { result } = renderHook(() => useSidebarSelection([defaultClaudeEntry(), managed('managed-1')]))
    expect(result.current.selectedId).toBe('default:claude')
  })

  it('falls back to entries[0] when no id is persisted', () => {
    const { result } = renderHook(() => useSidebarSelection([defaultClaudeEntry(), managed('managed-1')]))
    expect(result.current.selectedId).toBe('default:claude')
  })

  it('returns null when there are no entries', () => {
    const { result } = renderHook(() => useSidebarSelection([]))
    expect(result.current.selectedId).toBeNull()
  })

  it('select(id) updates state synchronously and writes to AppState after debounce', async () => {
    const { result } = renderHook(() => useSidebarSelection([defaultClaudeEntry(), managed('managed-1')]))
    act(() => {
      result.current.select('managed-1')
    })
    expect(result.current.selectedId).toBe('managed-1')
    expect(appStateFake.update).not.toHaveBeenCalled()
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300)
    })
    expect(appStateFake.update).toHaveBeenCalledWith({ selectedEntryId: 'managed-1' })
  })

  it('re-falls-back when the selected entry vanishes from the list', async () => {
    appStateFake.state.selectedEntryId = 'managed-1'
    const { result, rerender } = renderHook(
      ({ entries }: { entries: Array<SidebarEntry> }) => useSidebarSelection(entries),
      {
        initialProps: { entries: [defaultClaudeEntry(), managed('managed-1')] as Array<SidebarEntry> },
      },
    )
    expect(result.current.selectedId).toBe('managed-1')
    await act(async () => {
      rerender({ entries: [defaultClaudeEntry()] as Array<SidebarEntry> })
    })
    expect(result.current.selectedId).toBe('default:claude')
  })
})
