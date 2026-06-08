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

const allInstalled = {
  apps: {
    claude: { guiInstalled: true, cliInstalled: true },
    codex: { guiInstalled: false, cliInstalled: false },
  },
  localBinOnPath: true,
}

const noneInstalled = {
  apps: {
    claude: { guiInstalled: false, cliInstalled: false },
    codex: { guiInstalled: false, cliInstalled: false },
  },
  localBinOnPath: false,
}

describe('useDependencies', () => {
  it('reports all-installed state on success', async () => {
    mockInvoke.mockResolvedValueOnce(allInstalled)
    const { result } = renderHookWithQuery(() => useDependencies())
    await waitFor(() => expect(result.current).not.toBeNull())
    expect(result.current.deps).toEqual(allInstalled)
  })

  it('reports missing pieces', async () => {
    mockInvoke.mockResolvedValueOnce(noneInstalled)
    const { result } = renderHookWithQuery(() => useDependencies())
    await waitFor(() => expect(result.current).not.toBeNull())
    expect(result.current.deps.apps.claude.guiInstalled).toBe(false)
  })
})
