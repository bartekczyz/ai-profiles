import type { AppId, Session, SessionKind, SessionState } from '@/lib/types'
import type { SessionRowAction } from './session-row-actions'

import { Monitor, Terminal } from 'lucide-react'

import { cn, StatusDot } from '@/design'
import { shortenHomePath } from '@/features/profiles/components/shorten-home-path'

import { formatSessionLastUsed } from '../lib/format-session-last-used'
import { SessionRowActions } from './session-row-actions'

type Props = {
  /**
   * The session this row describes.
   */
  session: Session
  /**
   * The app the session is of.
   */
  app: AppId
  /**
   * What the row lets you do to the session. None renders no actions slot.
   */
  actions?: Array<SessionRowAction>
}

type KindPillProps = {
  /**
   * Where the session was started.
   */
  kind: SessionKind
}

type StateMarkerProps = {
  /**
   * What the session's files are doing right now.
   */
  state: SessionState
  /**
   * The app the session is of.
   */
  app: AppId
}

/**
 * What a session without a title is called.
 */
export const untitledSessionLabel = 'Untitled session'

/**
 * How a row names where its session was started.
 */
const kindLabels: Record<SessionKind, string> = {
  desktop: 'Desktop',
  cli: 'CLI',
}

/**
 * What an open session's marker says has it open, per app. Codex can't tell a
 * terminal from its other clients, so it says Codex has it open.
 */
const openStateTitles: Record<AppId, string> = {
  claude: 'Open in a terminal',
  codex: 'Codex has it open',
}

/**
 * One session: its title and where it was started on the first line; its
 * state, folder and age on the second, in the pane's mono metadata type; and
 * its actions trailing. A session whose transcript is gone is dimmed — there
 * is little left of it but the desktop record.
 *
 * The row carries no label of its own: its content names it, so a screen
 * reader announces the kind, state and folder along with the title.
 */
export function SessionRow({ session, app, actions = [] }: Props) {
  const title = session.title ?? untitledSessionLabel
  const lastUsed = formatSessionLastUsed(session.lastUsedAt)
  return (
    <li
      className={cn(
        'flex min-h-[46px] items-center gap-3 border-t border-border-soft px-[13px] py-[9px] first:border-t-0',
        session.state === 'transcriptMissing' && 'opacity-60',
      )}
    >
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <span
            className={cn(
              'truncate text-[12.5px] tracking-[-0.005em]',
              session.title === null ? 'text-muted-strong italic' : 'text-ink',
            )}
          >
            {title}
          </span>
          <KindPill kind={session.kind} />
        </div>
        {/* Each part after the first is preceded by a middle dot, so a
            missing folder or age leaves no dangling separator. */}
        <div className="mt-0.5 flex min-w-0 items-center font-mono text-[11px] text-muted-strong [&>*+*]:before:mx-1.5 [&>*+*]:before:text-border [&>*+*]:before:content-['·']">
          <StateMarker state={session.state} app={app} />
          {session.cwd === null ? null : (
            <span title={session.cwd} className="min-w-0 truncate">
              {shortenHomePath(session.cwd)}
            </span>
          )}
          {lastUsed === null ? null : <span className="shrink-0 whitespace-nowrap">{lastUsed}</span>}
        </div>
      </div>
      {actions.length > 0 ? <SessionRowActions actions={actions} /> : null}
    </li>
  )
}

/**
 * Desktop or CLI, as a small outlined tag beside the title.
 */
function KindPill({ kind }: KindPillProps) {
  const Icon = kind === 'desktop' ? Monitor : Terminal
  return (
    <span className="inline-flex shrink-0 items-center gap-1 rounded-[5px] border border-border px-1.5 py-px font-mono text-[9.5px] font-medium uppercase tracking-[0.08em] text-muted-strong">
      <Icon aria-hidden className="h-2.5 w-2.5" strokeWidth={2} />
      {kindLabels[kind]}
    </span>
  )
}

/**
 * Leads the metadata line when the session's state limits what can be done
 * with it: open (it can't be moved or archived until closed), saying what
 * has it open on hover, or its transcript deleted. Idle sessions, and
 * sessions held by a desktop app — which the app can quit for you — carry no
 * marker.
 */
function StateMarker({ state, app }: StateMarkerProps) {
  if (state === 'openInTerminal') {
    return (
      <span title={openStateTitles[app]} className="inline-flex shrink-0 items-center gap-1.5 text-green">
        <StatusDot pulse tone="success" />
        Open
      </span>
    )
  }
  if (state === 'transcriptMissing') {
    return <span className="shrink-0 whitespace-nowrap">Transcript deleted</span>
  }
  return null
}
