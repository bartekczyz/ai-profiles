import type { AppState, AppStatePatch } from '@/lib/types'

import { useMutation, useQueryClient, useSuspenseQuery } from '@tanstack/react-query'

import { loadAppState, updateAppState } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

type UseAppStateResult = {
  state: AppState
  update: (patch: AppStatePatch) => Promise<AppState>
  refresh: () => Promise<void>
}

export function useAppState(): UseAppStateResult {
  const queryClient = useQueryClient()
  const { data } = useSuspenseQuery({
    queryKey: queryKeys.appState,
    queryFn: loadAppState,
  })

  const mutation = useMutation({
    mutationFn: updateAppState,
    onMutate: async (patch) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.appState })
      const previous = queryClient.getQueryData<AppState>(queryKeys.appState)
      if (previous) {
        const optimistic: AppState = {
          ...previous,
          welcomeShown: patch.welcomeShown ?? previous.welcomeShown,
          migrationDismissedAt: patch.clearMigrationDismissed
            ? null
            : (patch.migrationDismissedAt ?? previous.migrationDismissedAt),
          pathBannerDismissedAt: patch.clearPathBannerDismissed
            ? null
            : (patch.pathBannerDismissedAt ?? previous.pathBannerDismissedAt),
          themeMode: patch.themeMode ?? previous.themeMode,
        }
        queryClient.setQueryData(queryKeys.appState, optimistic)
      }
      return { previous }
    },
    onError: (_error, _patch, context) => {
      if (context?.previous) {
        queryClient.setQueryData(queryKeys.appState, context.previous)
      }
    },
    onSettled: (next) => {
      if (next) {
        queryClient.setQueryData(queryKeys.appState, next)
      } else {
        void queryClient.invalidateQueries({ queryKey: queryKeys.appState })
      }
    },
  })

  return {
    state: data,
    update: (patch) => mutation.mutateAsync(patch),
    refresh: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.appState })
    },
  }
}
