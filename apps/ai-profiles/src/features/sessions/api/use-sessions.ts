import { useQuery } from '@tanstack/react-query'

import { listSessions } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

/**
 * The sessions a profile (or `default:<app>`) owns, active and archived.
 *
 * Sessions change outside the app — a terminal starts one, the desktop app
 * archives one — so the list refetches every time the window regains focus,
 * overriding the app-wide default. `'always'` rather than `true`: the global
 * 30s staleTime would otherwise skip a focus that comes soon after the last
 * fetch, and a session just started in a terminal wouldn't show up on return.
 * It stays in memory only: the persister keeps usage snapshots and nothing
 * else, so a restart always lists afresh.
 */
export function useSessions(profileId: string) {
  return useQuery({
    queryKey: queryKeys.sessions.list(profileId),
    queryFn: () => listSessions(profileId),
    refetchOnWindowFocus: 'always',
  })
}
