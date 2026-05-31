import type { Profile } from '@/lib/types'

import { invoke } from '@tauri-apps/api/core'
import { act, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { renderHookWithQuery } from '@/test/render-with-query'

import { useProfiles } from './use-profiles'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const mockInvoke = vi.mocked(invoke)

function profileFixture(overrides: Partial<Profile> = {}): Profile {
  return {
    id: '1',
    app: 'claude',
    name: 'Personal',
    slug: 'personal',
    color: '#d97757',
    createdAt: '2026-05-20T12:00:00Z',
    lastUsedAt: null,

    surfaces: { gui: true, cli: true },
    ...overrides,
  }
}

beforeEach(() => {
  mockInvoke.mockReset()
})

describe('useProfiles', () => {
  it('loads profiles on mount', async () => {
    const fixture = profileFixture()
    mockInvoke.mockResolvedValueOnce([fixture])

    const { result } = renderHookWithQuery(() => useProfiles())

    await waitFor(() => expect(result.current).not.toBeNull())
    expect(result.current.profiles).toEqual([fixture])
  })

  it('create appends the returned profile', async () => {
    mockInvoke.mockResolvedValueOnce([])
    const { result } = renderHookWithQuery(() => useProfiles())
    await waitFor(() => expect(result.current).not.toBeNull())

    const created = profileFixture({ id: '2', name: 'Work', slug: 'work' })
    mockInvoke.mockResolvedValueOnce(created)

    await act(async () => {
      await result.current.create({
        name: 'Work',
        color: '#6b8db5',
        surfaces: { gui: true, cli: false },
      })
    })

    await waitFor(() => expect(result.current.profiles).toEqual([created]))
  })

  it('update replaces the matching profile by id', async () => {
    const original = profileFixture()
    mockInvoke.mockResolvedValueOnce([original])
    const { result } = renderHookWithQuery(() => useProfiles())
    await waitFor(() => expect(result.current).not.toBeNull())

    const updated = { ...original, name: 'Renamed', slug: 'renamed' }
    mockInvoke.mockResolvedValueOnce(updated)

    await act(async () => {
      await result.current.update({ id: '1', patch: { name: 'Renamed' } })
    })

    await waitFor(() => expect(result.current.profiles).toEqual([updated]))
  })

  it('remove drops the profile from the list', async () => {
    const fixture = profileFixture()
    mockInvoke.mockResolvedValueOnce([fixture])
    const { result } = renderHookWithQuery(() => useProfiles())
    await waitFor(() => expect(result.current).not.toBeNull())

    mockInvoke.mockResolvedValueOnce(undefined)

    await act(async () => {
      await result.current.remove({ id: '1', moveToTrash: true })
    })

    await waitFor(() => expect(result.current.profiles).toEqual([]))
  })

  it('toggle replaces the profile with the server response', async () => {
    const original = profileFixture()
    mockInvoke.mockResolvedValueOnce([original])
    const { result } = renderHookWithQuery(() => useProfiles())
    await waitFor(() => expect(result.current).not.toBeNull())

    const toggled = { ...original, surfaces: { gui: false, cli: true } }
    mockInvoke.mockResolvedValueOnce(toggled)

    await act(async () => {
      await result.current.toggle({ id: '1', surface: 'gui', enabled: false })
    })

    await waitFor(() => expect(result.current.profiles[0].surfaces).toEqual({ gui: false, cli: true }))
  })

  it('reorder rewrites the cache in the requested sequence', async () => {
    const a = profileFixture({ id: 'a', name: 'A', slug: 'a' })
    const b = profileFixture({ id: 'b', name: 'B', slug: 'b' })
    const c = profileFixture({ id: 'c', name: 'C', slug: 'c' })
    mockInvoke.mockResolvedValueOnce([a, b, c])
    const { result } = renderHookWithQuery(() => useProfiles())
    await waitFor(() => expect(result.current).not.toBeNull())

    mockInvoke.mockResolvedValueOnce([c, a, b])

    await act(async () => {
      await result.current.reorder(['c', 'a', 'b'])
    })

    await waitFor(() => expect(result.current.profiles.map((profile) => profile.id)).toEqual(['c', 'a', 'b']))
    expect(mockInvoke).toHaveBeenLastCalledWith('reorder_profiles', { ids: ['c', 'a', 'b'] })
  })
})
