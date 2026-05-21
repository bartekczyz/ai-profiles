import type { AppError, ExistingInstallInfo, ImportExistingInput, Profile } from '@/lib/types'

import { useEffect, useState } from 'react'

import { detectExistingClaudeInstall, importExistingInstall } from '@/lib/commands'

type UseMigrationResult = {
  existing: ExistingInstallInfo | null
  loading: boolean
  error: string | null
  anyDetected: boolean
  import: (input: ImportExistingInput) => Promise<Profile>
  refresh: () => Promise<void>
}

export function useMigration(): UseMigrationResult {
  const [existing, setExisting] = useState<ExistingInstallInfo | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  async function refresh() {
    setError(null)
    try {
      const info = await detectExistingClaudeInstall()
      setExisting(info)
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

  async function performImport(input: ImportExistingInput) {
    return importExistingInstall(input)
  }

  const anyDetected = existing !== null && (existing.claudeDesktopPath !== null || existing.claudeCodePath !== null)

  return {
    existing,
    loading,
    error,
    anyDetected,
    import: performImport,
    refresh,
  }
}
