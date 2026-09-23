import type { AppId } from '@/lib/app-registry'
import type { Session, SessionAction } from '@/lib/types'

/**
 * Why a session a terminal has open can't be archived.
 */
export const closeInTerminalReason = 'Close it in the terminal first'

/**
 * An action a row offers, and why it is held back, if it is.
 */
export type RowAction = {
  /**
   * What the action does.
   */
  action: SessionAction
  /**
   * Why the action can't run right now. Present means disabled.
   */
  disabledReason?: string
}

/**
 * The actions `session`'s row offers: Archive on the Active tab, Restore on
 * the Archived one, for Claude and Codex sessions alike. Archiving waits for
 * a terminal that has the session open to close it; a desktop app in the way
 * is quit from the confirm dialog instead.
 */
export function rowActions(session: Session): Array<RowAction> {
  if (session.archived) {
    return [{ action: 'restore' }]
  }
  if (session.state === 'openInTerminal') {
    return [{ action: 'archive', disabledReason: closeInTerminalReason }]
  }
  return [{ action: 'archive' }]
}

/**
 * Whether a row offers Move, and why it is held back, if it is.
 */
export type MoveAvailability = {
  /**
   * Why the session can't move right now. Present means disabled.
   */
  disabledReason?: string
}

/**
 * Whether `session`'s row, in a profile of `app` with `targetCount` other
 * profiles of the app to move to, offers Move: only active Claude sessions,
 * when there is somewhere to go. A session that can't move keeps the action,
 * held back with the reason. `null` when the row offers no Move at all.
 */
export function moveAvailability(session: Session, app: AppId, targetCount: number): MoveAvailability | null {
  if (session.archived || app !== 'claude' || targetCount === 0) {
    return null
  }
  if (session.unmovableReason !== null) {
    return { disabledReason: session.unmovableReason }
  }
  return {}
}
