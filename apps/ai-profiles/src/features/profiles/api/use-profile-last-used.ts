import type { Profile } from '@/lib/types'

import { useMutation, useQueryClient } from '@tanstack/react-query'

import { useToast } from '@/design'
import { copyToClipboard, openProfileInApp, touchProfileLastUsed } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

function setEntry(list: Array<Profile> | undefined, updated: Profile): Array<Profile> {
  if (!list) {
    return [updated]
  }
  return list.map((profile) => (profile.id === updated.id ? updated : profile))
}

/**
 * Last-used events — launching the desktop app or copying the CLI command.
 * Each stamps lastUsedAt server-side and hands back the updated profile,
 * which we patch straight into the cached list so the header's
 * "Last used …" line refreshes without a refetch.
 *
 * A launch that had to leave out the profile's own Dock-icon launcher still
 * succeeded — the profile is open — so it surfaces as a notice rather than an
 * error, and the setting stays as it was.
 *
 * The hook returns plain async callbacks so the surface card can keep
 * its `onPrimary: () => void` interface; errors propagate to the caller.
 */
export function useProfileLastUsed(): {
  launchDesktop: (profileId: string) => Promise<Profile>
  copyCli: (input: { profileId: string; command: string }) => Promise<Profile>
} {
  const queryClient = useQueryClient()
  const toast = useToast()

  const launchMutation = useMutation({
    mutationFn: openProfileInApp,
    onSuccess: ({ profile, wrapperBypass }) => {
      queryClient.setQueryData<Array<Profile>>(queryKeys.profiles.all, (previous) => setEntry(previous, profile))
      if (wrapperBypass) {
        toast.info('Opened without its own Dock icon', wrapperBypass.reason)
      }
    },
  })

  const copyMutation = useMutation({
    mutationFn: async (input: { profileId: string; command: string }) => {
      await copyToClipboard(input.command)
      return touchProfileLastUsed(input.profileId)
    },
    onSuccess: (updated) => {
      queryClient.setQueryData<Array<Profile>>(queryKeys.profiles.all, (previous) => setEntry(previous, updated))
    },
  })

  return {
    launchDesktop: async (profileId) => (await launchMutation.mutateAsync(profileId)).profile,
    copyCli: (input) => copyMutation.mutateAsync(input),
  }
}
