/**
 * Format a byte count as a short human string ("248 MB", "1.4 GB").
 *
 * Uses binary units (1024) since these describe on-disk size — the same
 * unit the macOS Finder shows. One decimal place once the value crosses
 * 10 of the chosen unit; integer otherwise. Negative numbers and NaN
 * collapse to "0 B".
 */
const units = ['B', 'KB', 'MB', 'GB', 'TB'] as const

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return '0 B'
  }
  let value = bytes
  let unitIndex = 0
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }
  // Bytes are always integer; KB+ get one decimal under 10, integer at/above.
  if (unitIndex === 0) {
    return `${Math.round(value)} ${units[unitIndex]}`
  }
  const rounded = value < 10 ? value.toFixed(1) : Math.round(value).toString()
  return `${rounded} ${units[unitIndex]}`
}
