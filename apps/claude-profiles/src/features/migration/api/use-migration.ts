import type { ExistingInstallInfo, ExistingInstallSizes, ImportExistingInput, Profile } from '@/lib/types'

import { useMutation, useQuery, useQueryClient, useSuspenseQuery } from '@tanstack/react-query'

import { detectExistingClaudeInstall, detectExistingClaudeSizes, importExistingInstall } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

type UseMigrationResult = {
  existing: ExistingInstallInfo
  anyDetected: boolean
  import: (input: ImportExistingInput) => Promise<Profile>
  refresh: () => Promise<void>
}

export function useMigration(): UseMigrationResult {
  const queryClient = useQueryClient()
  const { data } = useSuspenseQuery({
    queryKey: queryKeys.migration.existing,
    queryFn: detectExistingClaudeInstall,
  })

  const importMutation = useMutation({
    mutationFn: importExistingInstall,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.profiles.all })
      void queryClient.invalidateQueries({ queryKey: queryKeys.migration.backups })
    },
  })

  const anyDetected = data.claudeDesktopPath !== null || data.claudeCodePath !== null

  return {
    existing: data,
    anyDetected,
    import: (input) => importMutation.mutateAsync(input),
    refresh: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.migration.existing })
    },
  }
}

/**
 * Lazy companion to `useMigration`. The path-existence query above runs
 * at boot; this one only fires when something subscribes — typically the
 * MigrationDialog when it opens — so the recursive `directory_size`
 * walks happen off the boot critical path. Returns `null` for both
 * sizes until the IPC resolves; the dialog shows the path without a
 * size in the meantime.
 */
export function useMigrationSizes(enabled: boolean): ExistingInstallSizes {
  const { data } = useQuery({
    queryKey: queryKeys.migration.sizes,
    queryFn: detectExistingClaudeSizes,
    enabled,
    staleTime: 60_000,
  })
  return data ?? { claudeDesktopSizeBytes: null, claudeCodeSizeBytes: null }
}
