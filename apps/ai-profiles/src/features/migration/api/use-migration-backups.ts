import type { MigrationBackupInfo } from '@/lib/types'

import { useMutation, useQueryClient, useSuspenseQuery } from '@tanstack/react-query'

import { deleteMigrationBackup, listMigrationBackups } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

type UseMigrationBackupsResult = {
  backups: Array<MigrationBackupInfo>
  remove: (path: string) => Promise<void>
  refresh: () => Promise<void>
}

export function useMigrationBackups(): UseMigrationBackupsResult {
  const queryClient = useQueryClient()
  const { data } = useSuspenseQuery({
    queryKey: queryKeys.migration.backups,
    queryFn: listMigrationBackups,
  })

  const removeMutation = useMutation({
    mutationFn: deleteMigrationBackup,
    onMutate: async (path) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.migration.backups })
      const previous = queryClient.getQueryData<Array<MigrationBackupInfo>>(queryKeys.migration.backups)
      if (previous) {
        queryClient.setQueryData(
          queryKeys.migration.backups,
          previous.filter((backup) => backup.path !== path),
        )
      }
      return { previous }
    },
    onError: (_error, _path, context) => {
      if (context?.previous) {
        queryClient.setQueryData(queryKeys.migration.backups, context.previous)
      }
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.migration.backups })
    },
  })

  return {
    backups: data,
    remove: (path) => removeMutation.mutateAsync(path).then(() => undefined),
    refresh: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.migration.backups })
    },
  }
}
