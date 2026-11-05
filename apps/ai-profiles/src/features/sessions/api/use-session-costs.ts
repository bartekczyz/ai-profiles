import { useQuery } from '@tanstack/react-query'

import { getSessionCosts } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

/**
 * What each session of a profile (or `default:<app>`) would cost at API prices,
 * by its id.
 *
 * Read apart from the list, so the rows show before their costs: pricing
 * reads every response the transcripts hold. Like the list, it is read again
 * every time the window regains focus, which stays cheap as the backend reads
 * on only from where each transcript was last read to, and it stays in memory
 * only.
 */
export function useSessionCosts(profileId: string) {
  return useQuery({
    queryKey: queryKeys.sessions.costs(profileId),
    queryFn: () => getSessionCosts(profileId),
    refetchOnWindowFocus: 'always',
  })
}
