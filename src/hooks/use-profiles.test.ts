import type { Profile } from '@/lib/types'

import { invoke } from '@tauri-apps/api/core'
import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { useProfiles } from './use-profiles'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const mockInvoke = vi.mocked(invoke)

function profileFixture(overrides: Partial<Profile> = {}): Profile {
  return {
    id: '1',
    name: 'Personal',
    slug: 'personal',
    color: '#d97757',
    createdAt: '2026-05-20T12:00:00Z',
    surfaces: { gui: true, cli: true },
    ...overrides,
  }
}

beforeEach(() => {
  mockInvoke.mockReset()
})

describe('useProfiles', () => {
  it('loads profiles on mount and selects the first one', async () => {
    const fixture = profileFixture()
    mockInvoke.mockResolvedValueOnce([fixture])

    const { result } = renderHook(() => useProfiles())

    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })
    expect(result.current.profiles).toEqual([fixture])
    expect(result.current.selectedId).toBe('1')
    expect(result.current.error).toBe(null)
  })

  it('captures errors from list_profiles into state', async () => {
    mockInvoke.mockRejectedValueOnce({ kind: 'Io', message: 'boom' })

    const { result } = renderHook(() => useProfiles())

    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })
    expect(result.current.error).toBe('boom')
    expect(result.current.profiles).toEqual([])
  })

  it('create appends the returned profile and selects it', async () => {
    mockInvoke.mockResolvedValueOnce([])
    const { result } = renderHook(() => useProfiles())
    await waitFor(() => expect(result.current.loading).toBe(false))

    const created = profileFixture({ id: '2', name: 'Work', slug: 'work' })
    mockInvoke.mockResolvedValueOnce(created)

    await act(async () => {
      await result.current.create({
        name: 'Work',
        color: '#6b8db5',
        surfaces: { gui: true, cli: false },
      })
    })

    expect(result.current.profiles).toEqual([created])
    expect(result.current.selectedId).toBe('2')
  })

  it('update replaces the matching profile by id', async () => {
    const original = profileFixture()
    mockInvoke.mockResolvedValueOnce([original])
    const { result } = renderHook(() => useProfiles())
    await waitFor(() => expect(result.current.loading).toBe(false))

    const updated = { ...original, name: 'Renamed', slug: 'renamed' }
    mockInvoke.mockResolvedValueOnce(updated)

    await act(async () => {
      await result.current.update({ id: '1', patch: { name: 'Renamed' } })
    })

    expect(result.current.profiles).toEqual([updated])
  })

  it('remove drops the profile and clears selection when it was selected', async () => {
    const fixture = profileFixture()
    mockInvoke.mockResolvedValueOnce([fixture])
    const { result } = renderHook(() => useProfiles())
    await waitFor(() => expect(result.current.loading).toBe(false))

    mockInvoke.mockResolvedValueOnce(undefined)

    await act(async () => {
      await result.current.remove({ id: '1', moveToTrash: true })
    })

    expect(result.current.profiles).toEqual([])
    expect(result.current.selectedId).toBe(null)
  })

  it('toggle replaces the profile with the server response', async () => {
    const original = profileFixture()
    mockInvoke.mockResolvedValueOnce([original])
    const { result } = renderHook(() => useProfiles())
    await waitFor(() => expect(result.current.loading).toBe(false))

    const toggled = { ...original, surfaces: { gui: false, cli: true } }
    mockInvoke.mockResolvedValueOnce(toggled)

    await act(async () => {
      await result.current.toggle({ id: '1', surface: 'gui', enabled: false })
    })

    expect(result.current.profiles[0].surfaces).toEqual({ gui: false, cli: true })
  })
})
