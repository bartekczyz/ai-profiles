import type { Period } from '@/lib/types'

import { useQuery } from '@tanstack/react-query'

import { getProfileApiCost } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

/**
 * An estimate of what a profile's Claude Code or Codex use over `period`
 * would cost at API prices.
 *
 * Transcripts grow outside the app, so it is read again every time the window
 * regains focus, as the sessions list is. That stays cheap: the backend reads
 * on only from where each transcript was last read to. It stays in memory
 * only, like everything but usage snapshots. While another period of the same
 * profile is read, the last one stays, so the breakdown doesn't blink out.
 */
export function useProfileApiCost(profileId: string, period: Period) {
  return useQuery({
    queryKey: queryKeys.profileApiCost(profileId, period),
    queryFn: () => getProfileApiCost(profileId, period),
    refetchOnWindowFocus: 'always',
    placeholderData: (previous, previousQuery) => (previousQuery?.queryKey[1] === profileId ? previous : undefined),
  })
}
