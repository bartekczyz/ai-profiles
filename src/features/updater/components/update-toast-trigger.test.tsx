// SPDX-License-Identifier: MIT

import { check } from '@tauri-apps/plugin-updater'
import { render, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'

import { UpdateToastTrigger } from './update-toast-trigger'

vi.mock('@tauri-apps/plugin-updater', () => ({ check: vi.fn() }))

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

const mockCheck = vi.mocked(check)

beforeEach(() => {
  mockCheck.mockReset()
  showSpy.mockReset()
})

describe('UpdateToastTrigger', () => {
  it('does not toast when up-to-date', async () => {
    mockCheck.mockResolvedValueOnce(null)
    render(
      <ToastProvider>
        <UpdateToastTrigger />
      </ToastProvider>,
    )
    await waitFor(() => expect(mockCheck).toHaveBeenCalled())
    expect(showSpy).not.toHaveBeenCalled()
  })

  it('toasts when an update is available', async () => {
    mockCheck.mockResolvedValueOnce({
      version: '0.2.0',
      currentVersion: '0.1.0',
      downloadAndInstall: vi.fn(),
    } as unknown as Awaited<ReturnType<typeof check>>)
    render(
      <ToastProvider>
        <UpdateToastTrigger />
      </ToastProvider>,
    )
    await waitFor(() => expect(showSpy).toHaveBeenCalled())
    const input = showSpy.mock.calls[0][0]
    expect(input).toMatchObject({
      tone: 'info',
      title: 'Update available',
      description: expect.stringContaining('0.2.0'),
      durationMs: null,
      action: expect.objectContaining({ label: 'Restart and install' }),
    })
  })

  it('does not double-toast for the same version', async () => {
    mockCheck.mockResolvedValueOnce({
      version: '0.2.0',
      currentVersion: '0.1.0',
      downloadAndInstall: vi.fn(),
    } as unknown as Awaited<ReturnType<typeof check>>)
    const { rerender } = render(
      <ToastProvider>
        <UpdateToastTrigger />
      </ToastProvider>,
    )
    await waitFor(() => expect(showSpy).toHaveBeenCalledTimes(1))
    rerender(
      <ToastProvider>
        <UpdateToastTrigger />
      </ToastProvider>,
    )
    expect(showSpy).toHaveBeenCalledTimes(1)
  })
})
