import type { ProfilePaths } from '@/lib/types'

import { useSuspenseQuery } from '@tanstack/react-query'

import { profilePaths as fetchProfilePaths } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

/**
 * Loads the on-disk paths for a profile (data dir, launcher path, wrapper
 * path, CLI config dir). Keyed per profile id; refreshes only via explicit
 * cache invalidation since these paths are derived from the profile slug
 * and don't change while the profile exists.
 */
export function useProfilePaths(id: string): ProfilePaths {
  const { data } = useSuspenseQuery({
    queryKey: queryKeys.profiles.paths(id),
    queryFn: () => fetchProfilePaths(id),
  })
  return data
}
