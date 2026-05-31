import { invoke } from '@tauri-apps/api/core'
import { act, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { renderHookWithQuery } from '@/test/render-with-query'

import { useMigration } from './use-migration'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const mockInvoke = vi.mocked(invoke)

beforeEach(() => {
  mockInvoke.mockReset()
})

describe('useMigration', () => {
  it('loads detection result on mount and reports anyDetected', async () => {
    mockInvoke.mockResolvedValueOnce({
      guiPath: '/Users/me/Library/Application Support/Claude',
      cliPath: null,
      guiSizeBytes: null,
      cliSizeBytes: null,
    })

    const { result } = renderHookWithQuery(() => useMigration())

    await waitFor(() => expect(result.current).not.toBeNull())
    expect(result.current.existing.guiPath).toMatch(/Claude$/)
    expect(result.current.anyDetected).toBe(true)
  })

  it('reports anyDetected=false when neither path was found', async () => {
    mockInvoke.mockResolvedValueOnce({ guiPath: null, cliPath: null, guiSizeBytes: null, cliSizeBytes: null })

    const { result } = renderHookWithQuery(() => useMigration())

    await waitFor(() => expect(result.current).not.toBeNull())
    expect(result.current.anyDetected).toBe(false)
  })

  it('import passes the input through to invoke', async () => {
    mockInvoke.mockResolvedValueOnce({ guiPath: '/x', cliPath: null, guiSizeBytes: null, cliSizeBytes: null })
    const { result } = renderHookWithQuery(() => useMigration())
    await waitFor(() => expect(result.current).not.toBeNull())

    const fakeProfile = {
      id: '1',
      app: 'claude',
      name: 'Default',
      slug: 'default',
      color: '#d97757',
      createdAt: '2026-05-20T12:00:00Z',
      lastUsedAt: null,

      surfaces: { gui: true, cli: false },
    }
    mockInvoke.mockResolvedValueOnce(fakeProfile)

    let returned: unknown
    await act(async () => {
      returned = await result.current.import({
        name: 'Default',
        color: '#d97757',
        includeGui: true,
        includeCli: false,
      })
    })
    expect(returned).toEqual(fakeProfile)
    expect(mockInvoke).toHaveBeenLastCalledWith('import_existing_install', {
      app: 'claude',
      input: { name: 'Default', color: '#d97757', includeGui: true, includeCli: false },
    })
  })
})
