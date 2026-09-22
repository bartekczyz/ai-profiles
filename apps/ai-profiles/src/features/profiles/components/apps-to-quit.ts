import type { AppToQuit } from '@/lib/types'

/**
 * "Claude (Work)", or "Claude (Work) and Claude (Default)": the apps a move or
 * archive will quit, as a button and its note name them.
 */
export function appsToQuitLabel(apps: ReadonlyArray<AppToQuit>): string {
  return apps.map((app) => `Claude (${app.label})`).join(' and ')
}
