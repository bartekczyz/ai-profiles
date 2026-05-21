import type { AppError, MigrationBackupInfo } from '@/lib/types'

import { useEffect, useState } from 'react'

import { deleteMigrationBackup, listMigrationBackups } from '@/lib/commands'

type UseMigrationBackupsResult = {
  backups: Array<MigrationBackupInfo>
  loading: boolean
  error: string | null
  remove: (path: string) => Promise<void>
  refresh: () => Promise<void>
}

export function useMigrationBackups(): UseMigrationBackupsResult {
  const [backups, setBackups] = useState<Array<MigrationBackupInfo>>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  async function refresh() {
    setError(null)
    try {
      setBackups(await listMigrationBackups())
    } catch (caught) {
      setError((caught as AppError).message ?? String(caught))
    }
  }

  // biome-ignore lint/correctness/useExhaustiveDependencies: refresh is intentionally only called on mount
  useEffect(() => {
    void refresh().finally(() => {
      setLoading(false)
    })
  }, [])

  async function remove(path: string) {
    await deleteMigrationBackup(path)
    setBackups((previous) => previous.filter((backup) => backup.path !== path))
  }

  return { backups, loading, error, remove, refresh }
}
