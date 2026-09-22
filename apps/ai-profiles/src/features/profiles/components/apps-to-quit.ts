import type { AppToQuit } from '@/lib/types'

/**
 * "Claude (Work)", or "Claude (Work) and Claude (Default)": the apps a move or
 * archive will quit, as a button and its note name them.
 */
export function appsToQuitLabel(apps: ReadonlyArray<AppToQuit>): string {
  return apps.map((app) => `Claude (${app.label})`).join(' and ')
}

/**
 * The note under a move or archive that will quit `apps` first: which apps,
 * and what that costs.
 */
export function appsToQuitNote(apps: ReadonlyArray<AppToQuit>): string {
  const one = apps.length === 1
  return `${appsToQuitLabel(apps)} will quit first, since ${one ? 'it has' : 'they have'} the session open or ${
    one ? 'keeps' : 'keep'
  } the session list being changed. ${one ? 'Its' : 'Their'} other sessions close too, and come back when you open ${
    one ? 'it' : 'them'
  } again.`
}
