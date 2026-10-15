import { useQuery } from '@tanstack/react-query'

import { getProfileApiCost } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

/**
 * What a profile's Claude Code use over the last 30 days would have cost at
 * Claude API prices.
 *
 * Transcripts grow outside the app, so it is read again every time the window
 * regains focus, as the sessions list is. That stays cheap: the backend reads
 * on only from where each transcript was last read to. It stays in memory
 * only, like everything but usage snapshots.
 */
export function useProfileApiCost(profileId: string, enabled: boolean) {
  return useQuery({
    queryKey: queryKeys.profileApiCost(profileId),
    queryFn: () => getProfileApiCost(profileId),
    enabled,
    refetchOnWindowFocus: 'always',
  })
}
