import type { AppId, Session, SessionAction } from '@/lib/types'

/**
 * Why an open session of each app can't be archived. Codex can't tell a
 * terminal from its other clients, so it says Codex has it open.
 */
const openSessionReasons: Record<AppId, string> = {
  claude: 'Close it in the terminal first',
  codex: 'Codex has it open — close it first',
}

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
 * The actions `session`'s row, a session of `app`, offers: Archive on the
 * Active tab, Restore on the Archived one, for Claude and Codex sessions
 * alike. Archiving waits for a terminal that has the session open to close
 * it; a desktop app in the way is quit from the confirm dialog instead.
 */
export function rowActions(session: Session, app: AppId): Array<RowAction> {
  if (session.archived) {
    return [{ action: 'restore' }]
  }
  if (session.state === 'openInTerminal') {
    return [{ action: 'archive', disabledReason: openSessionReasons[app] }]
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
 * Whether `session`'s row, with `targetCount` other profiles of its app to
 * move to, offers Move: active sessions of either app, when there is somewhere
 * to go. A session that can't move keeps the action, held back with the
 * reason. `null` when the row offers no Move at all.
 */
export function moveAvailability(session: Session, targetCount: number): MoveAvailability | null {
  if (session.archived || targetCount === 0) {
    return null
  }
  if (session.unmovableReason !== null) {
    return { disabledReason: session.unmovableReason }
  }
  return {}
}
