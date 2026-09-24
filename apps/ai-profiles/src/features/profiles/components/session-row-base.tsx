import type { ReactNode } from 'react'

import { formatDistanceToNow } from 'date-fns'

import { shortenHomePath } from './shorten-home-path'

/** The inset panel a session list sits in. */
export const sessionPanelClasses = 'rounded-[10px] border border-border-soft bg-white/30 dark:bg-white/[0.02]'

export const sessionRowClasses =
  'flex min-h-[46px] items-center justify-between gap-3 border-t border-border-soft px-[13px] py-[8px] first:border-t-0'

type Props = {
  title: string
  /** Dimmed, for sessions that aren't live (archived). */
  muted?: boolean
  /** Beside the title: a surface pill, an Open status. */
  badges?: ReactNode
  folder: string | null
  /** The home folder paths are shortened against; the Mac's when omitted. */
  home?: string
  /** RFC 3339, shown relative ("2 hours ago"). */
  at: string | null
  /** Words before the time, e.g. "archived". */
  atLabel?: string
  /** The right-hand side: buttons, or a status. */
  actions?: ReactNode
}

/**
 * One row of a session list: title and badges over folder and time, with
 * actions on the right. Shared by the profile's own sessions, its archive,
 * and a remote host's sessions, so all three line up.
 */
export function SessionRowBase({ title, muted, badges, folder, home, at, atLabel, actions }: Props) {
  return (
    <li className={sessionRowClasses}>
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5">
          <span className={`truncate text-body ${muted ? 'text-ink-soft' : 'text-ink'}`} title={title}>
            {title}
          </span>
          {badges}
        </div>
        <div className="flex min-w-0 font-mono text-mono text-muted-strong">
          <span className="truncate" title={folder ?? undefined}>
            {folder ? shortenHomePath(folder, home) : 'unknown folder'}
          </span>
          {at ? (
            <span className="shrink-0 whitespace-nowrap">
              <span className="mx-1.5 text-border">·</span>
              {atLabel ? `${atLabel} ` : ''}
              {formatDistanceToNow(new Date(at), { addSuffix: true })}
            </span>
          ) : null}
        </div>
      </div>
      {actions}
    </li>
  )
}
