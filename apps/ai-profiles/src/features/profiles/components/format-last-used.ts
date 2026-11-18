import { formatDistanceToNow } from 'date-fns'

/**
 * A profile used more recently than this is in use: no "Last used" to show.
 */
const inUseMs = 5 * 60 * 1000

/**
 * Renders the "Last used …" sub-line for the managed-profile detail
 * header. Returns "Never used" for null timestamps or unparseable strings,
 * and null for a use within the last five minutes, which isn't worth saying.
 */
export function formatLastUsed(timestamp: string | null): string | null {
  if (!timestamp) {
    return 'Never used'
  }
  const parsed = new Date(timestamp)
  if (Number.isNaN(parsed.getTime())) {
    return 'Never used'
  }
  if (Date.now() - parsed.getTime() < inUseMs) {
    return null
  }
  return `Last used ${formatDistanceToNow(parsed, { addSuffix: true })}`
}
