import { invoke } from '@tauri-apps/api/core'
import { act, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { renderHookWithQuery } from '@/test/render-with-query'

import { useAppState } from './use-app-state'

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
