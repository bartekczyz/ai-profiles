import type { AppId, Session, SessionCost, SessionKind, SessionState } from '@/lib/types'
import type { SessionRowAction } from './session-row-actions'

import { Fragment } from 'react'

import { Monitor, TerminalWindow } from '@phosphor-icons/react'

import { cn, Skeleton, StatusDot } from '@/design'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/design/ui/tooltip'
import { shortenHomePath } from '@/features/profiles/components/shorten-home-path'
import { formatUsd } from '@/lib/format-cost'

import { formatSessionLastUsed } from '../lib/format-session-last-used'
import { shownUsd } from '../lib/session-cost'
import { SessionRowActions } from './session-row-actions'

type Props = {
  /**
   * Whether the session's cost is still being read.
   */
  costPending: boolean
  /**
   * The session this row describes.
   */
  session: Session
  /**
   * The app the session is of.
   */
  app: AppId
  /**
   * What the session would cost at API prices; none when it used nothing, or
   * the costs couldn't be read.
   */
  cost?: SessionCost
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

type CostCellProps = {
  /**
   * Whether the cost is still being read.
   */
  pending: boolean
  /**
   * The app the session is of.
   */
  app: AppId
  /**
   * What the session would cost; none when there is nothing to show.
   */
  cost?: SessionCost
}

type CostBreakdownProps = {
  /**
   * The cost to break down.
   */
  cost: SessionCost
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
 * A row of the list, its column heads included: the session, its cost and
 * its actions, in the list's columns. Where subgrid is supported the columns
 * are the list's own, sized by every row at once, so the costs line up with
 * their head whatever each figure's width; elsewhere each row sizes its own,
 * which still lines the figures up on their right edge, as every row of a tab
 * offers the same actions.
 */
export const sessionRowGridClasses =
  'col-span-full grid grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-x-3 px-[13px] supports-[grid-template-columns:subgrid]:grid-cols-subgrid'

/**
 * Whose list prices a cost's breakdown is at, per app.
 */
const priceLists: Record<AppId, string> = {
  claude: 'Claude API',
  codex: 'OpenAI API',
}

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
 * state, folder and age on the second, in the pane's mono metadata type; its
 * cost at API prices in the cost column; and its actions trailing. A session
 * whose transcript is gone is dimmed — there is little left of it but the
 * desktop record.
 *
 * The row carries no label of its own: its content names it, so a screen
 * reader announces the kind, state, folder and cost along with the title.
 */
export function SessionRow({ costPending, session, app, cost, actions = [] }: Props) {
  const title = session.title ?? untitledSessionLabel
  const lastUsed = formatSessionLastUsed(session.lastUsedAt)
  const missing = session.state === 'transcriptMissing'
  return (
    <li
      className={cn(
        sessionRowGridClasses,
        'min-h-[46px] border-t border-border-soft py-[9px] first:border-t-0',
        missing && 'opacity-60',
      )}
    >
      <div className="min-w-0">
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
      {/* A session whose transcript is gone has no use left to price. */}
      <CostCell pending={costPending && !missing} app={app} cost={cost} />
      <div className="flex justify-end">{actions.length > 0 ? <SessionRowActions actions={actions} /> : null}</div>
    </li>
  )
}

/**
 * Desktop or CLI, as a small outlined tag beside the title.
 */
function KindPill({ kind }: KindPillProps) {
  const Icon = kind === 'desktop' ? Monitor : TerminalWindow
  return (
    <span className="inline-flex shrink-0 items-center gap-1 rounded-[5px] border border-border px-1.5 py-px text-[11px] font-medium text-muted-strong">
      <Icon aria-hidden className="h-2.5 w-2.5" />
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

/**
 * The session's API-equivalent cost, right-aligned in the cost column: its
 * total, with a `+` when some of its use couldn't be priced, or `—` when none
 * could; a placeholder while it is read; nothing when there is none. Under a
 * dollar it is muted, so the sessions that cost something stand out. Hovering
 * breaks it down.
 */
function CostCell({ pending, app, cost }: CostCellProps) {
  if (pending) {
    return <Skeleton shape="text" className="h-2.5 w-11 justify-self-end" />
  }
  if (cost === undefined) {
    return <span />
  }
  const usd = shownUsd(cost)
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          className={cn(
            'justify-self-end font-mono text-[12px] whitespace-nowrap tabular-nums',
            usd !== null && usd >= 1 ? 'text-ink-soft' : 'text-muted-strong',
          )}
        >
          <span className="sr-only">API-equivalent </span>
          {usd === null ? '—' : formatUsd(usd)}
          {cost.coverage === 'partial' ? (
            <sup className="ml-px text-[10px] text-amber">
              +<span className="sr-only"> unpriced use</span>
            </sup>
          ) : null}
        </span>
      </TooltipTrigger>
      <TooltipContent sideOffset={4} className="px-2 py-1.5 font-mono text-[11px] leading-[1.5]">
        <CostBreakdown cost={cost} app={app} />
      </TooltipContent>
    </Tooltip>
  )
}

/**
 * What a cost is made of: each model's share, most expensive first, then the
 * use left out for want of a price. It is an API-equivalent: the same figure
 * whether the plan covered the use or it was paid for, which no transcript
 * records.
 */
function CostBreakdown({ cost, app }: CostBreakdownProps) {
  return (
    <div className="grid max-w-64 grid-cols-[auto_auto] gap-x-4">
      <span className="col-span-2 mb-0.5 opacity-70">API-equivalent, at {priceLists[app]} list prices</span>
      {cost.byModel.map((model) => (
        <Fragment key={model.model}>
          <span>{model.model}</span>
          <span className="text-right tabular-nums">{formatUsd(model.usd)}</span>
        </Fragment>
      ))}
      {cost.unpriced.map((use) => (
        <span key={`${use.model}:${use.missing}`} className="col-span-2 mt-0.5 text-amber">
          {formatTokens(use.tokens)} tokens of {use.model} left out: no {use.missing ?? 'price'} for it
        </span>
      ))}
    </div>
  )
}

/**
 * A token count, shortened: `1.9M`, `310k`, `950`.
 */
function formatTokens(tokens: number): string {
  if (tokens >= 1_000_000) {
    return `${(tokens / 1_000_000).toFixed(1)}M`
  }
  if (tokens >= 1_000) {
    return `${Math.round(tokens / 1_000)}k`
  }
  return `${tokens}`
}
