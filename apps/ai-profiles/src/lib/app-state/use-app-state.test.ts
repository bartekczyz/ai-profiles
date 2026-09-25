import type { AppState } from '@/lib/types'

import { invoke } from '@tauri-apps/api/core'
import { act, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { renderHookWithQuery } from '@/test/render-with-query'

import { computeOptimisticAppState, useAppState } from './use-app-state'

function makeAppState(overrides: Partial<AppState> = {}): AppState {
  return {
    welcomeShown: false,
    migrationDismissedAt: null,
    pathBannerDismissedAt: null,
    themeMode: 'system',
    selectedEntryId: null,
    dockIconAcknowledgedAt: null,
    defaultProfileNames: {},
    dismissedRepairSessions: {},
    ...overrides,
  }
}

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const mockInvoke = vi.mocked(invoke)

beforeEach(() => {
  mockInvoke.mockReset()
})

describe('useAppState', () => {
  it('loads state on mount', async () => {
    mockInvoke.mockResolvedValueOnce({
      welcomeShown: true,
      migrationDismissedAt: null,
      pathBannerDismissedAt: null,
      themeMode: 'system',
    })

    const { result } = renderHookWithQuery(() => useAppState())
    await waitFor(() => expect(result.current).not.toBeNull())
    expect(result.current.state.welcomeShown).toBe(true)
  })

  it('update passes the patch through invoke and replaces state', async () => {
    mockInvoke.mockResolvedValueOnce({
      welcomeShown: false,
      migrationDismissedAt: null,
      pathBannerDismissedAt: null,
      themeMode: 'system',
    })
    const { result } = renderHookWithQuery(() => useAppState())
    await waitFor(() => expect(result.current).not.toBeNull())

    mockInvoke.mockResolvedValueOnce({
      welcomeShown: true,
      migrationDismissedAt: null,
      pathBannerDismissedAt: null,
      themeMode: 'system',
    })

    await act(async () => {
      await result.current.update({ welcomeShown: true })
    })

    expect(mockInvoke).toHaveBeenLastCalledWith('update_app_state', {
      patch: { welcomeShown: true },
    })
    await waitFor(() => expect(result.current.state.welcomeShown).toBe(true))
  })

  it('passes the Dock icon acknowledgement through and reflects it in state', async () => {
    mockInvoke.mockResolvedValueOnce({
      welcomeShown: true,
      migrationDismissedAt: null,
      pathBannerDismissedAt: null,
      themeMode: 'system',
      selectedEntryId: null,
      dockIconAcknowledgedAt: null,
    })
    const { result } = renderHookWithQuery(() => useAppState())
    await waitFor(() => expect(result.current).not.toBeNull())
    expect(result.current.state.dockIconAcknowledgedAt).toBeNull()

    const acknowledgedAt = '2026-05-21T09:30:00.000Z'
    mockInvoke.mockResolvedValueOnce({
      welcomeShown: true,
      migrationDismissedAt: null,
      pathBannerDismissedAt: null,
      themeMode: 'system',
      selectedEntryId: null,
      dockIconAcknowledgedAt: acknowledgedAt,
    })

    await act(async () => {
      await result.current.update({ dockIconAcknowledgedAt: acknowledgedAt })
    })

    expect(mockInvoke).toHaveBeenLastCalledWith('update_app_state', {
      patch: { dockIconAcknowledgedAt: acknowledgedAt },
    })
    await waitFor(() => expect(result.current.state.dockIconAcknowledgedAt).toBe(acknowledgedAt))
  })
})

describe('computeOptimisticAppState', () => {
  it('overrides welcomeShown and themeMode when patched', () => {
    const previous = makeAppState({ welcomeShown: false, themeMode: 'system' })
    const result = computeOptimisticAppState(previous, { welcomeShown: true, themeMode: 'dark' })
    expect(result.welcomeShown).toBe(true)
    expect(result.themeMode).toBe('dark')
  })

  it('keeps prior fields untouched when the patch omits them', () => {
    const previous = makeAppState({ welcomeShown: true, themeMode: 'light' })
    const result = computeOptimisticAppState(previous, {})
    expect(result.welcomeShown).toBe(true)
    expect(result.themeMode).toBe('light')
  })

  it('sets migrationDismissedAt from the patch', () => {
    const previous = makeAppState({ migrationDismissedAt: null })
    const result = computeOptimisticAppState(previous, { migrationDismissedAt: '2026-01-01T00:00:00.000Z' })
    expect(result.migrationDismissedAt).toBe('2026-01-01T00:00:00.000Z')
  })

  it('nulls migrationDismissedAt when clearMigrationDismissed is set, even alongside a value', () => {
    const previous = makeAppState({ migrationDismissedAt: '2026-01-01T00:00:00.000Z' })
    const result = computeOptimisticAppState(previous, {
      migrationDismissedAt: '2026-02-01T00:00:00.000Z',
      clearMigrationDismissed: true,
    })
    expect(result.migrationDismissedAt).toBeNull()
  })

  it('sets pathBannerDismissedAt from the patch and clears it on the clear flag', () => {
    const previous = makeAppState({ pathBannerDismissedAt: null })
    const set = computeOptimisticAppState(previous, { pathBannerDismissedAt: '2026-01-01T00:00:00.000Z' })
    expect(set.pathBannerDismissedAt).toBe('2026-01-01T00:00:00.000Z')
    const cleared = computeOptimisticAppState(set, { clearPathBannerDismissed: true })
    expect(cleared.pathBannerDismissedAt).toBeNull()
  })

  it('sets selectedEntryId from the patch and clears it on the clear flag', () => {
    const previous = makeAppState({ selectedEntryId: null })
    const set = computeOptimisticAppState(previous, { selectedEntryId: 'entry-1' })
    expect(set.selectedEntryId).toBe('entry-1')
    const cleared = computeOptimisticAppState(set, { clearSelectedEntryId: true })
    expect(cleared.selectedEntryId).toBeNull()
  })

  it('records dockIconAcknowledgedAt from the patch', () => {
    const previous = makeAppState({ dockIconAcknowledgedAt: null })
    const result = computeOptimisticAppState(previous, { dockIconAcknowledgedAt: '2026-05-21T09:30:00.000Z' })
    expect(result.dockIconAcknowledgedAt).toBe('2026-05-21T09:30:00.000Z')
  })

  it('sets a default profile name and drops it again for an empty name', () => {
    const previous = makeAppState({ defaultProfileNames: {} })
    const renamed = computeOptimisticAppState(previous, { defaultProfileName: { app: 'claude', name: 'Work' } })
    expect(renamed.defaultProfileNames).toEqual({ claude: 'Work' })
    const restored = computeOptimisticAppState(renamed, { defaultProfileName: { app: 'claude', name: '  ' } })
    expect(restored.defaultProfileNames).toEqual({})
  })

  it('sets dismissed repair sessions and forgets the dismissal for an empty list', () => {
    const previous = makeAppState({ dismissedRepairSessions: {} })
    const dismissed = computeOptimisticAppState(previous, {
      dismissedRepair: { profileId: 'p1', sessionIds: ['s1', 's2'] },
    })
    expect(dismissed.dismissedRepairSessions).toEqual({ p1: ['s1', 's2'] })
    const forgotten = computeOptimisticAppState(dismissed, {
      dismissedRepair: { profileId: 'p1', sessionIds: [] },
    })
    expect(forgotten.dismissedRepairSessions).toEqual({})
  })
})
