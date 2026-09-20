export const lastSeenVersionKey = 'ai-profiles-last-seen-version'

/**
 * Version recorded on the previous launch, or `null` when nothing is stored or storage is
 * unavailable.
 */
export function readLastSeenVersion(): string | null {
  try {
    return window.localStorage.getItem(lastSeenVersionKey)
  } catch {
    return null
  }
}

/**
 * Records the running version. Failures are swallowed: losing the marker only means the upgrade
 * toast may repeat on the next launch.
 */
export function writeLastSeenVersion(version: string): void {
  try {
    window.localStorage.setItem(lastSeenVersionKey, version)
  } catch {
    // Storage can be blocked or full; nothing useful to do.
  }
}
