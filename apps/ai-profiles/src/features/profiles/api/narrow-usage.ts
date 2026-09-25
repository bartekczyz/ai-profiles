import type { ProfileUsage, QuotaError, QuotaUsage, RateLimitResetCredits, Spend, UsageWindow } from '@/lib/types'

const knownQuotaErrors: ReadonlyArray<QuotaError> = [
  'no_credentials',
  'unauthorized',
  'forbidden',
  'needs_login',
  'rate_limited',
  'network',
  'unknown',
]

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

/**
 * True when `value` is a number, a safe integer, and at least `min`. Shared
 * by the narrowing functions below, which repeatedly need to check that an
 * untrusted IPC field is a usable integer before trusting it.
 */
function isSafeIntegerAtLeast(value: unknown, min: number): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= min
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
  const resets = narrowResetCredits(input.rateLimitResetCredits)
  const spend = narrowSpend(input.spend)
  return {
    ...(resets ? { rateLimitResetCredits: resets } : {}),
    ...(spend ? { spend } : {}),
    primary: narrowWindow(input.primary),
    secondary: narrowWindow(input.secondary),
    scopedWeekly: Array.isArray(input.scopedWeekly)
      ? input.scopedWeekly.map(narrowWindow).filter((window) => window !== null)
      : [],
  }
}

/**
 * Amounts must be safe integers in minor units for the formatter to be
 * trustworthy — a float or NaN here would render a nonsense price, so the
 * whole row is dropped instead.
 */
export function narrowSpend(input: unknown): Spend | undefined {
  if (!isRecord(input) || typeof input.currency !== 'string' || !input.currency) {
    return
  }
  if (!isSafeIntegerAtLeast(input.usedMinor, 0)) {
    return
  }
  const limitMinor = isSafeIntegerAtLeast(input.limitMinor, 1) ? input.limitMinor : null
  // Kept inside Intl's fraction-digit range; two places is the safe
  // assumption for every currency Anthropic bills in.
  const exponent = isSafeIntegerAtLeast(input.exponent, 0) && input.exponent <= 6 ? input.exponent : 2
  const percent =
    typeof input.percent === 'number' && Number.isFinite(input.percent) && input.percent >= 0 ? input.percent : null
  return { usedMinor: input.usedMinor, currency: input.currency, exponent, limitMinor, percent }
}

function narrowResetCredits(input: unknown): RateLimitResetCredits | undefined {
  if (!isRecord(input) || !isSafeIntegerAtLeast(input.availableCount, 0)) {
    return
  }
  const credits = Array.isArray(input.credits)
    ? input.credits.filter(isRecord).map((credit) => ({
        title: typeof credit.title === 'string' ? credit.title : null,
        status: typeof credit.status === 'string' ? credit.status : 'unknown',
        expiresAt:
          isSafeIntegerAtLeast(credit.expiresAt, 0) && !Number.isNaN(new Date(credit.expiresAt * 1000).getTime())
            ? credit.expiresAt
            : null,
      }))
    : null
  return { availableCount: input.availableCount, credits }
}

export function narrowWindow(input: unknown): UsageWindow | null {
  if (!isRecord(input)) {
    return null
  }
  // Utilization is a percentage on the 0..=100 scale. We don't clamp
  // the upper bound — over-limit values (e.g. 105%) are legitimate and
  // the renderer caps the visual bar separately.
  const raw = input.utilization
  const utilization = typeof raw === 'number' && Number.isFinite(raw) && raw >= 0 ? raw : null
  const resetsAt = typeof input.resetsAt === 'string' ? input.resetsAt : null
  const windowDurationMins = isSafeIntegerAtLeast(input.windowDurationMins, 1) ? input.windowDurationMins : undefined
  const label = typeof input.label === 'string' && input.label.trim() ? input.label.trim() : undefined
  return {
    utilization,
    resetsAt,
    ...(windowDurationMins === undefined ? {} : { windowDurationMins }),
    ...(label === undefined ? {} : { label }),
  }
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
