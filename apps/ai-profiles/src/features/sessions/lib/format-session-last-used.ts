import { formatDistanceToNow } from 'date-fns'

/**
 * How long ago a session was last used ("12 minutes ago"), or null when the
 * timestamp can't be read — the row then leaves the age out rather than
 * inventing one.
 */
export function formatSessionLastUsed(timestamp: string): string | null {
  const parsed = new Date(timestamp)
  if (Number.isNaN(parsed.getTime())) {
    return null
  }
  return formatDistanceToNow(parsed, { addSuffix: true })
}
