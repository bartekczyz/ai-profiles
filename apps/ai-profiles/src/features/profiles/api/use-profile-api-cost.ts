import type { Period } from '@/lib/types'
import type { LimitCycle } from '../lib/limit-cycle'

import { useQuery } from '@tanstack/react-query'

import { getProfileApiCost, getProfileApiCostBetween } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

/**
 * An estimate of what a profile's Claude Code or Codex use over `period`, or
 * over a limit cycle, would cost at API prices.
 *
 * Transcripts grow outside the app, so it is read again every time the window
 * regains focus, as the sessions list is. That stays cheap: the backend reads
 * on only from where each transcript was last read to. It stays in memory
 * only, like everything but usage snapshots. While another period of the same
 * profile is read, the last one stays, so the breakdown doesn't blink out.
 * Without a period, as for a profile with no known cycle, nothing is read.
 */
export function useProfileApiCost(profileId: string, period: Period | LimitCycle | null) {
  return useQuery({
    queryKey: costKey(profileId, period),
    queryFn: () => {
      if (period === null) {
        throw new Error('No period to read the cost of')
      }
      return typeof period === 'string'
        ? getProfileApiCost(profileId, period)
        : getProfileApiCostBetween(profileId, period.from.toISOString(), period.to.toISOString())
    },
    enabled: period !== null,
    refetchOnWindowFocus: 'always',
    placeholderData: (previous, previousQuery) => (previousQuery?.queryKey[1] === profileId ? previous : undefined),
  })
}

function costKey(profileId: string, period: Period | LimitCycle | null) {
  if (period === null) {
    return ['profile-api-cost', profileId] as const
  }
  return typeof period === 'string'
    ? queryKeys.profileApiCost(profileId, period)
    : queryKeys.profileApiCostBetween(profileId, period.from.toISOString(), period.to.toISOString())
}
