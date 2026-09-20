import type { AppMetadata } from '@/lib/types'

import { waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { getAppMetadata } from '@/lib/commands'
import { renderHookWithQuery } from '@/test/render-with-query'

import { lastSeenVersionKey } from './last-seen-version'
import { useWhatsNew } from './use-whats-new'

vi.mock('@/lib/commands', () => ({ getAppMetadata: vi.fn() }))

function makeMetadata(version: string): AppMetadata {
  return { name: 'ai-profiles', version, description: '', authors: [], repository: null, homepage: null, license: null }
}

beforeEach(() => {
  window.localStorage.clear()
  vi.mocked(getAppMetadata).mockReset()
  vi.mocked(getAppMetadata).mockResolvedValue(makeMetadata('1.1.0'))
})

// These use the real bundled CHANGELOG.md; only versions <= the mocked current one (1.1.0) matter,
// and those entries are historical, so the assertions stay stable across future releases.
describe('useWhatsNew', () => {
  it('reports an upgrade with every skipped release and keeps that snapshot after recording the version', async () => {
    window.localStorage.setItem(lastSeenVersionKey, '1.0.1')

    const { result, rerender } = renderHookWithQuery(() => useWhatsNew())

    await waitFor(() => expect(window.localStorage.getItem(lastSeenVersionKey)).toBe('1.1.0'))
    expect(result.current.version).toBe('1.1.0')
    expect(result.current.upgraded).toBe(true)
    expect(result.current.releases.map((release) => release.version)).toEqual(['1.1.0', '1.0.2'])

    // The stored version now equals the running one; a re-render must not recompute the decision.
    rerender()

    expect(result.current.upgraded).toBe(true)
    expect(result.current.releases.map((release) => release.version)).toEqual(['1.1.0', '1.0.2'])
  })

  it('stays quiet on a fresh install but records the version', async () => {
    const { result } = renderHookWithQuery(() => useWhatsNew())

    await waitFor(() => expect(window.localStorage.getItem(lastSeenVersionKey)).toBe('1.1.0'))
    expect(result.current.upgraded).toBe(false)
  })

  it('stays quiet when the version is unchanged', async () => {
    window.localStorage.setItem(lastSeenVersionKey, '1.1.0')

    const { result } = renderHookWithQuery(() => useWhatsNew())

    await waitFor(() => expect(result.current.version).toBe('1.1.0'))
    expect(result.current.upgraded).toBe(false)
  })

  it('does not crash when storage throws', async () => {
    const getItemSpy = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('denied')
    })
    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('denied')
    })

    try {
      const { result } = renderHookWithQuery(() => useWhatsNew())

      await waitFor(() => expect(result.current.version).toBe('1.1.0'))
      expect(result.current.upgraded).toBe(false)
    } finally {
      getItemSpy.mockRestore()
      setItemSpy.mockRestore()
    }
  })
})
