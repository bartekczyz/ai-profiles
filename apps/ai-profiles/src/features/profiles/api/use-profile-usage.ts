import type { ProfileUsage, QuotaError, QuotaUsage, UsageWindow } from '@/lib/types'

import { useQuery } from '@tanstack/react-query'

import { getProfileUsage } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

export const refetchIntervalMs = 5 * 60 * 1000
const knownQuotaErrors: ReadonlyArray<QuotaError> = [
  'no_credentials',
  'unauthorized',
  'forbidden',
  'needs_login',
  'rate_limited',
  'network',
  'unknown',
]

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

function safeEmpty(): ProfileUsage {
  return {
    quota: null,
    quotaError: 'unknown',
    fetchedAt: new Date().toISOString(),
  }
}

/**
 * Defensive narrowing in case the backend shape drifts (e.g. a new
 * QuotaError variant, a missing field, NaN utilization). Anything
 * that doesn't match falls back to safe-empty fields rather than
 * crashing the card.
 */
export function narrowProfileUsage(input: unknown): ProfileUsage {
  if (!isRecord(input)) {
    return safeEmpty()
  }
  return {
    quota: narrowQuota(input.quota),
    quotaError: narrowQuotaError(input.quotaError),
    fetchedAt: typeof input.fetchedAt === 'string' ? input.fetchedAt : new Date().toISOString(),
  }
}

function narrowQuota(input: unknown): QuotaUsage | null {
  if (!isRecord(input)) {
    return null
  }
  return {
    primary: narrowWindow(input.primary),
    secondary: narrowWindow(input.secondary),
    secondaryExtra: narrowWindow(input.secondaryExtra),
  }
}

function narrowWindow(input: unknown): UsageWindow | null {
  if (!isRecord(input)) {
    return null
  }
  // Utilization is a percentage on the 0..=100 scale. We don't clamp
  // the upper bound — over-limit values (e.g. 105%) are legitimate and
  // the renderer caps the visual bar separately.
  const raw = input.utilization
  const utilization = typeof raw === 'number' && Number.isFinite(raw) && raw >= 0 ? raw : null
  const resetsAt = typeof input.resetsAt === 'string' ? input.resetsAt : null
  return { utilization, resetsAt }
}

function narrowQuotaError(input: unknown): QuotaError | null {
  if (input === null || input === undefined) {
    return null
  }
  if (typeof input !== 'string') {
    return 'unknown'
  }
  if ((knownQuotaErrors as ReadonlyArray<string>).includes(input)) {
    return input as QuotaError
  }
  return 'unknown'
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
