import { invoke } from '@tauri-apps/api/core'
import { waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { renderHookWithQuery } from '@/test/render-with-query'

import { useDependencies } from './use-dependencies'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const mockInvoke = vi.mocked(invoke)

beforeEach(() => {
  mockInvoke.mockReset()
})

describe('useDependencies', () => {
  it('reports all-installed state on success', async () => {
    mockInvoke.mockResolvedValueOnce({
      claudeAppInstalled: true,
      claudeCliInstalled: true,
      localBinOnPath: true,
    })
    const { result } = renderHookWithQuery(() => useDependencies())
    await waitFor(() => expect(result.current).not.toBeNull())
    expect(result.current.deps).toEqual({
      claudeAppInstalled: true,
      claudeCliInstalled: true,
      localBinOnPath: true,
    })
  })

  it('reports missing pieces', async () => {
    mockInvoke.mockResolvedValueOnce({
      claudeAppInstalled: false,
      claudeCliInstalled: false,
      localBinOnPath: false,
    })
    const { result } = renderHookWithQuery(() => useDependencies())
    await waitFor(() => expect(result.current).not.toBeNull())
    expect(result.current.deps.claudeAppInstalled).toBe(false)
  })
})
