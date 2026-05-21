import { invoke } from '@tauri-apps/api/core'
import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { useMigration } from './use-migration'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const mockInvoke = vi.mocked(invoke)

beforeEach(() => {
  mockInvoke.mockReset()
})

describe('useMigration', () => {
  it('loads detection result on mount and reports anyDetected', async () => {
    mockInvoke.mockResolvedValueOnce({
      claudeDesktopPath: '/Users/me/Library/Application Support/Claude',
      claudeCodePath: null,
    })

    const { result } = renderHook(() => useMigration())

    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.existing?.claudeDesktopPath).toMatch(/Claude$/)
    expect(result.current.anyDetected).toBe(true)
  })

  it('reports anyDetected=false when neither path was found', async () => {
    mockInvoke.mockResolvedValueOnce({ claudeDesktopPath: null, claudeCodePath: null })

    const { result } = renderHook(() => useMigration())

    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.anyDetected).toBe(false)
  })

  it('surfaces backend errors as a string', async () => {
    mockInvoke.mockRejectedValueOnce({ kind: 'Io', message: 'permission denied' })

    const { result } = renderHook(() => useMigration())

    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.error).toBe('permission denied')
  })

  it('import passes the input through to invoke', async () => {
    mockInvoke.mockResolvedValueOnce({ claudeDesktopPath: '/x', claudeCodePath: null })
    const { result } = renderHook(() => useMigration())
    await waitFor(() => expect(result.current.loading).toBe(false))

    const fakeProfile = {
      id: '1',
      name: 'Default',
      slug: 'default',
      color: '#7C3AED',
      createdAt: '2026-05-20T12:00:00Z',
      surfaces: { gui: true, cli: false },
    }
    mockInvoke.mockResolvedValueOnce(fakeProfile)

    let returned: unknown
    await act(async () => {
      returned = await result.current.import({
        name: 'Default',
        color: '#7C3AED',
        includeGui: true,
        includeCli: false,
      })
    })
    expect(returned).toEqual(fakeProfile)
    expect(mockInvoke).toHaveBeenLastCalledWith('import_existing_install', {
      input: { name: 'Default', color: '#7C3AED', includeGui: true, includeCli: false },
    })
  })
})
