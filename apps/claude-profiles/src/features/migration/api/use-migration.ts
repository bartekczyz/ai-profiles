import type { AppId } from '@/lib/app-registry'
import type { ExistingInstallInfo, ExistingInstallSizes, ImportExistingInput, Profile } from '@/lib/types'

import { useMutation, useQuery, useQueryClient, useSuspenseQuery } from '@tanstack/react-query'

import { appIds } from '@/lib/app-registry'
import { detectExistingInstall, detectExistingSizes, importExistingInstall } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

/**
 * Pure: the apps with a detected stock install available to import (a GUI
 * and/or CLI path present), in `appIds` order. Drives every "Detect and
 * import" entry point (command palette, Settings, ⌘I) so a Codex install is
 * reachable, not just Claude.
 */
export function importableAppsFrom(existingByApp: Record<AppId, ExistingInstallInfo>): Array<AppId> {
  return appIds.filter((appId) => {
    const existing = existingByApp[appId]
    return existing.guiPath !== null || existing.cliPath !== null
  })
}

type UseMigrationResult = {
  existing: ExistingInstallInfo
  anyDetected: boolean
  import: (input: ImportExistingInput) => Promise<Profile>
  refresh: () => Promise<void>
}

export function useMigration(app: AppId = 'claude'): UseMigrationResult {
  const queryClient = useQueryClient()
  const { data } = useSuspenseQuery({
    queryKey: [...queryKeys.migration.existing, app],
    queryFn: () => detectExistingInstall(app),
  })

  const importMutation = useMutation({
    mutationFn: (input: ImportExistingInput) => importExistingInstall(app, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.profiles.all })
      void queryClient.invalidateQueries({ queryKey: queryKeys.migration.backups })
    },
  })

  const anyDetected = data.guiPath !== null || data.cliPath !== null

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
export function useMigrationSizes(enabled: boolean, app: AppId = 'claude'): ExistingInstallSizes {
  const { data } = useQuery({
    queryKey: [...queryKeys.migration.sizes, app],
    queryFn: () => detectExistingSizes(app),
    enabled,
    staleTime: 60_000,
  })
  return data ?? { guiSizeBytes: null, cliSizeBytes: null }
}
