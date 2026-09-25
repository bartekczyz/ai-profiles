import type { ProfileUsage, QuotaError } from '@/lib/types'

import { useQuery } from '@tanstack/react-query'

import { getProfileUsage } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

import { narrowProfileUsage } from './narrow-usage'

export const refetchIntervalMs = 5 * 60 * 1000

/**
 * Raised when a usage fetch succeeds at the IPC layer but carries no usable
 * quota (a `quotaError`, or a missing quota). We throw it — rather than
 * returning it as `data` — so React Query keeps the last successful snapshot
 * in `data` (which cross-restart persistence relies on) and surfaces the
 * failure separately via `error`.
 */
export class UsageUnavailableError extends Error {
  readonly code: QuotaError

  constructor(code: QuotaError) {
    super(`usage unavailable: ${code}`)
    this.name = 'UsageUnavailableError'
    this.code = code
  }
}

/**
 * Pure: passes a usable usage snapshot through, or throws
 * `UsageUnavailableError` carrying the reason. A null quota with no explicit
 * error is treated as `unknown` so empty meters are never presented as
 * success.
 */
export function ensureUsable(usage: ProfileUsage): ProfileUsage {
  if (usage.quotaError) {
    throw new UsageUnavailableError(usage.quotaError)
  }
  if (!usage.quota) {
    throw new UsageUnavailableError('unknown')
  }
  return usage
}

/**
 * Fetches the profile's usage stats. Refetches every 5 minutes while the
 * query is active and on every mount, so opening the detail page always
 * triggers a fresh fetch. A fetch problem is thrown as `UsageUnavailableError`
 * (not returned as data) so React Query retains the last good snapshot.
 */
export function useProfileUsage(profileId: string) {
  return useQuery({
    queryKey: queryKeys.profileUsage(profileId),
    queryFn: async () => ensureUsable(narrowProfileUsage(await getProfileUsage(profileId))),
    staleTime: 0,
    refetchOnMount: 'always',
    refetchInterval: refetchIntervalMs,
    // A usage failure (rate limit, sign-in needed) won't resolve on an
    // immediate retry, and the Rust backend already de-dupes and backs off;
    // retrying here would only double the upstream calls.
    retry: false,
  })
}
