import { invoke } from '@tauri-apps/api/core'
import { act, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { renderHookWithQuery } from '@/test/render-with-query'

import { useMigrationBackups } from './use-migration-backups'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const mockInvoke = vi.mocked(invoke)

beforeEach(() => {
  mockInvoke.mockReset()
})

describe('useMigrationBackups', () => {
  it('loads backups on mount', async () => {
    mockInvoke.mockResolvedValueOnce([
      {
        path: '/x/migration-backup-1',
        createdAtMs: 1000,
        sizeBytes: 1024,
        eligibleForCleanup: true,
      },
    ])
    const { result } = renderHookWithQuery(() => useMigrationBackups())
    await waitFor(() => expect(result.current).not.toBeNull())
    expect(result.current.backups).toHaveLength(1)
    expect(result.current.backups[0].eligibleForCleanup).toBe(true)
  })

  it('removes a backup optimistically when remove() resolves', async () => {
    mockInvoke.mockResolvedValueOnce([
      {
        path: '/x/migration-backup-1',
        createdAtMs: 1000,
        sizeBytes: 1024,
        eligibleForCleanup: true,
      },
      {
        path: '/x/migration-backup-2',
        createdAtMs: 2000,
        sizeBytes: 2048,
        eligibleForCleanup: false,
      },
    ])
    const { result } = renderHookWithQuery(() => useMigrationBackups())
    await waitFor(() => expect(result.current).not.toBeNull())

    mockInvoke.mockResolvedValueOnce(undefined)
    // refetch after invalidation returns just the remaining backup
    mockInvoke.mockResolvedValueOnce([
      {
        path: '/x/migration-backup-2',
        createdAtMs: 2000,
        sizeBytes: 2048,
        eligibleForCleanup: false,
      },
    ])

    await act(async () => {
      await result.current.remove('/x/migration-backup-1')
    })

    await waitFor(() => expect(result.current.backups).toHaveLength(1))
    expect(result.current.backups[0].path).toBe('/x/migration-backup-2')
  })
})
