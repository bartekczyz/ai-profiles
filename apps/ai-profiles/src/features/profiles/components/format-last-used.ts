import { formatDistanceToNow } from 'date-fns'

/**
 * Renders the "Last used …" sub-line for the managed-profile detail
 * header. Returns "Never used" for null timestamps or unparseable strings
 * so the header always has a stable two-clause sub-line.
 */
export function formatLastUsed(timestamp: string | null): string {
  if (!timestamp) {
    return 'Never used'
  }
  const parsed = new Date(timestamp)
  if (Number.isNaN(parsed.getTime())) {
    return 'Never used'
  }
  return `Last used ${formatDistanceToNow(parsed, { addSuffix: true })}`
}
