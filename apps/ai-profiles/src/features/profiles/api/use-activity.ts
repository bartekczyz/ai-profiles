import type { Activity } from '@/lib/types'

import { useSuspenseQuery } from '@tanstack/react-query'

import { listActivity } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

/**
 * Loads the most recent activity entries for a profile, newest first.
 * Limit defaults to 3 (matching the detail-pane timeline). The activity
 * cache is keyed per profile id so switching profiles doesn't blow away
 * the entries we already have for the previous one.
 */
export function useProfileActivity(profileId: string, limit = 3): Array<Activity> {
  const { data } = useSuspenseQuery({
    queryKey: queryKeys.profiles.activity(profileId),
    queryFn: () => listActivity({ profileId, limit }),
  })
  return data
}
