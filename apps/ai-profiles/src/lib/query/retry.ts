import { extractErrorKind } from '@/lib/extract-error-message'

/**
 * Whether a query that failed `failureCount` times, last with `error`, is
 * tried again: once, unless the tool it needs isn't installed. Trying again
 * can't change that, and would only hold back the notice saying so.
 */
export function retryUnlessNotInstalled(failureCount: number, error: unknown): boolean {
  return extractErrorKind(error) !== 'NotInstalled' && failureCount < 1
}
