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
        queryClient.setQueryData(queryKeys.appState, computeOptimisticAppState(previous, patch))
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

/**
 * Computes the optimistic `AppState` shown while a patch is in flight, by
 * applying the patch's overrides (and "clear" flags) on top of the last
 * known state. Pure so the mutation's `onMutate` stays a thin orchestrator.
 */
export function computeOptimisticAppState(previous: AppState, patch: AppStatePatch): AppState {
  return {
    ...previous,
    welcomeShown: patch.welcomeShown ?? previous.welcomeShown,
    migrationDismissedAt: patch.clearMigrationDismissed
      ? null
      : (patch.migrationDismissedAt ?? previous.migrationDismissedAt),
    pathBannerDismissedAt: patch.clearPathBannerDismissed
      ? null
      : (patch.pathBannerDismissedAt ?? previous.pathBannerDismissedAt),
    themeMode: patch.themeMode ?? previous.themeMode,
    selectedEntryId: patch.clearSelectedEntryId ? null : (patch.selectedEntryId ?? previous.selectedEntryId),
    dockIconAcknowledgedAt: patch.dockIconAcknowledgedAt ?? previous.dockIconAcknowledgedAt,
    defaultProfileNames: withDefaultProfileName(previous.defaultProfileNames, patch.defaultProfileName),
    dismissedRepairSessions: withDismissedRepair(previous.dismissedRepairSessions, patch.dismissedRepair),
  }
}

/** Mirrors the Rust side: trims, and an empty name drops the custom name. */
function withDefaultProfileName(
  names: AppState['defaultProfileNames'] | undefined,
  rename: AppStatePatch['defaultProfileName'],
): AppState['defaultProfileNames'] {
  const next = { ...(names ?? {}) }
  if (rename === undefined) {
    return next
  }
  const name = rename.name.trim()
  if (name.length === 0) {
    delete next[rename.app]
  } else {
    next[rename.app] = name
  }
  return next
}

/**
 * Mirrors the Rust side: an empty list forgets the profile's dismissal.
 */
function withDismissedRepair(
  dismissed: AppState['dismissedRepairSessions'] | undefined,
  patch: AppStatePatch['dismissedRepair'],
): AppState['dismissedRepairSessions'] {
  const next = { ...(dismissed ?? {}) }
  if (patch === undefined) {
    return next
  }
  if (patch.sessionIds.length === 0) {
    delete next[patch.profileId]
  } else {
    next[patch.profileId] = patch.sessionIds
  }
  return next
}
