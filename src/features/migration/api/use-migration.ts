import type { ExistingInstallInfo, ImportExistingInput, Profile } from '@/lib/types'

import { useMutation, useQueryClient, useSuspenseQuery } from '@tanstack/react-query'

import { detectExistingClaudeInstall, importExistingInstall } from '@/lib/commands'
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
