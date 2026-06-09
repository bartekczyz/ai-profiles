// SPDX-License-Identifier: MIT

import { check } from '@tauri-apps/plugin-updater'
import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { useUpdater } from './use-updater'

vi.mock('@tauri-apps/plugin-updater', () => ({
  check: vi.fn(),
}))

const mockCheck = vi.mocked(check)

beforeEach(() => {
  mockCheck.mockReset()
  vi.useFakeTimers({ shouldAdvanceTime: true })
})

afterEach(() => {
  vi.useRealTimers()
})

describe('useUpdater', () => {
  it('reports up-to-date when check returns null', async () => {
    mockCheck.mockResolvedValueOnce(null)
    const { result } = renderHook(() => useUpdater())
    await waitFor(() => expect(result.current.status.kind).toBe('up-to-date'))
  })

  it('reports available when check returns an Update', async () => {
    const fakeUpdate = {
      version: '0.2.0',
      currentVersion: '0.1.0',
      downloadAndInstall: vi.fn().mockResolvedValue(undefined),
    } as unknown as Awaited<ReturnType<typeof check>>
    mockCheck.mockResolvedValueOnce(fakeUpdate)
    const { result } = renderHook(() => useUpdater())
    await waitFor(() => expect(result.current.status.kind).toBe('available'))
    if (result.current.status.kind === 'available') {
      expect(result.current.status.update.version).toBe('0.2.0')
    }
  })

  it('reports error when check throws', async () => {
    mockCheck.mockRejectedValueOnce(new Error('network down'))
    const { result } = renderHook(() => useUpdater())
    await waitFor(() => expect(result.current.status.kind).toBe('error'))
    if (result.current.status.kind === 'error') {
      expect(result.current.status.message).toBe('network down')
    }
  })

  it('installAndRestart calls downloadAndInstall when available', async () => {
    const downloadAndInstall = vi.fn().mockResolvedValue(undefined)
    mockCheck.mockResolvedValueOnce({
      version: '0.2.0',
      currentVersion: '0.1.0',
      downloadAndInstall,
    } as unknown as Awaited<ReturnType<typeof check>>)
    const { result } = renderHook(() => useUpdater())
    await waitFor(() => expect(result.current.status.kind).toBe('available'))

    await act(async () => {
      await result.current.installAndRestart()
    })

    expect(downloadAndInstall).toHaveBeenCalled()
  })

  it('rechecks on the 6-hour interval', async () => {
    mockCheck.mockResolvedValue(null)
    renderHook(() => useUpdater())
    await waitFor(() => expect(mockCheck).toHaveBeenCalledTimes(1))

    await act(async () => {
      vi.advanceTimersByTime(6 * 60 * 60 * 1000)
    })

    await waitFor(() => expect(mockCheck).toHaveBeenCalledTimes(2))
  })
})
