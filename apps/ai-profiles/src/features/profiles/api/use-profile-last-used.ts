import type { Profile } from '@/lib/types'

import { useMutation, useQueryClient } from '@tanstack/react-query'

import { copyToClipboard, openProfileInApp, recordActivity } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

function setEntry(list: Array<Profile> | undefined, updated: Profile): Array<Profile> {
  if (!list) {
    return [updated]
  }
  return list.map((profile) => (profile.id === updated.id ? updated : profile))
}

/**
 * Last-used events — launching the desktop app or copying the CLI command.
 * Each updates lastUsedAt server-side and appends an activity entry, so
 * after the mutation we refresh both the profile list and that profile's
 * activity timeline.
 *
 * The hook returns plain async callbacks so the surface card can keep
 * its `onPrimary: () => void` interface; errors propagate to the caller.
 */
export function useProfileLastUsed(): {
  launchDesktop: (profileId: string) => Promise<Profile>
  copyCli: (input: { profileId: string; command: string }) => Promise<Profile>
} {
  const queryClient = useQueryClient()

  const launchMutation = useMutation({
    mutationFn: openProfileInApp,
    onSuccess: (updated) => {
      queryClient.setQueryData<Array<Profile>>(queryKeys.profiles.all, (previous) => setEntry(previous, updated))
      void queryClient.invalidateQueries({ queryKey: queryKeys.profiles.activity(updated.id) })
    },
  })

  const copyMutation = useMutation({
    mutationFn: async (input: { profileId: string; command: string }) => {
      await copyToClipboard(input.command)
      return recordActivity({ profileId: input.profileId, kind: 'copied_cli' })
    },
    onSuccess: (updated) => {
      queryClient.setQueryData<Array<Profile>>(queryKeys.profiles.all, (previous) => setEntry(previous, updated))
      void queryClient.invalidateQueries({ queryKey: queryKeys.profiles.activity(updated.id) })
    },
  })

  return {
    launchDesktop: (profileId) => launchMutation.mutateAsync(profileId),
    copyCli: (input) => copyMutation.mutateAsync(input),
  }
}
