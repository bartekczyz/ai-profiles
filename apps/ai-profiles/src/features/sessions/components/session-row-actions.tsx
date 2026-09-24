import { useId, useState } from 'react'

import { MoreHorizontal } from 'lucide-react'

import { cn } from '@/design'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/design/ui/dropdown-menu'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/design/ui/tooltip'

/**
 * Somewhere an action can be pointed at, like a profile to move a session to.
 */
export type SessionRowActionTarget = {
  /**
   * Stable key among the action's targets, passed to its `onSelect`.
   */
  id: string
  /**
   * The target's name in the menu.
   */
  label: string
}

/**
 * One thing a row lets you do to its session.
 */
export type SessionRowAction = {
  /**
   * Stable key among the row's actions.
   */
  id: string
  /**
   * The button's text.
   */
  label: string
  /**
   * Why the action can't run right now. Present means disabled: the action
   * stays on screen, greyed out, and says why.
   */
  disabledReason?: string
  /**
   * Where the action can be pointed. Present means choosing the action opens
   * a menu of these, and picking one runs it.
   */
  targets?: Array<SessionRowActionTarget>
  /**
   * Runs the action, pointed at the target picked, if it has targets.
   */
  onSelect: (targetId?: string) => void
}

type Props = {
  /**
   * The row's actions, in display order.
   */
  actions: Array<SessionRowAction>
}

type ActionProps = {
  /**
   * The action this control runs.
   */
  action: SessionRowAction
}

type HeldBackReasonProps = {
  /**
   * The id the button points its description at.
   */
  id: string
  /**
   * Why the action can't run right now; absent while it can.
   */
  reason?: string
}

/**
 * A row action's trigger, inline or the ⋯ menu: borderless until hovered,
 * dimmed while the action is unavailable.
 */
const controlClasses =
  'inline-flex h-7 shrink-0 cursor-pointer items-center rounded-[7px] text-[12px] text-muted outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:bg-ink/[0.06] hover:text-ink focus-visible:ring-2 focus-visible:ring-orange/40 aria-disabled:cursor-default aria-disabled:opacity-50 aria-disabled:hover:bg-transparent aria-disabled:hover:text-muted'

/**
 * A row's actions, laid out for the room the panel has: side by side as
 * buttons once the panel is at least 480px wide, folded into a ⋯ menu below
 * that. Both are rendered and a container query on the panel shows one, so
 * the switch needs no measuring. Needs a `TooltipProvider` above it, shared
 * by every row of the list.
 */
export function SessionRowActions({ actions }: Props) {
  return (
    <>
      <div className="hidden shrink-0 items-center gap-0.5 @min-[480px]/sessions:flex">
        {actions.map((action) =>
          action.targets === undefined ? (
            <InlineAction key={action.id} action={action} />
          ) : (
            <InlineTargetsAction key={action.id} action={action} />
          ),
        )}
      </div>
      <div className="shrink-0 @min-[480px]/sessions:hidden">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button type="button" aria-label="Session actions" className={cn(controlClasses, 'w-7 justify-center')}>
              <MoreHorizontal aria-hidden className="h-4 w-4" />
            </button>
          </DropdownMenuTrigger>
          {/* The generated content pins its width to the 28px trigger; the
              reasons under disabled items need room to read. */}
          <DropdownMenuContent align="end" className="w-56">
            {actions.map((action) => (
              <MenuAction key={action.id} action={action} />
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </>
  )
}

/**
 * A side-by-side action button. A disabled one stays focusable
 * (`aria-disabled` rather than `disabled`) so its reason reaches a screen
 * reader as the description and shows as a tooltip on hover or focus. The
 * button sits in the same tooltip whether it is held back or not, with the
 * tooltip kept shut while there is no reason, so a button that is held back
 * or let go while focused keeps its focus.
 */
function InlineAction({ action }: ActionProps) {
  const reasonId = useId()
  const [tooltipOpen, setTooltipOpen] = useState(false)
  const disabled = action.disabledReason !== undefined
  return (
    <Tooltip open={disabled && tooltipOpen} onOpenChange={setTooltipOpen}>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-disabled={disabled}
          aria-describedby={disabled ? reasonId : undefined}
          className={cn(controlClasses, 'px-2')}
          onClick={() => {
            if (!disabled) {
              action.onSelect()
            }
          }}
        >
          {action.label}
        </button>
      </TooltipTrigger>
      <HeldBackReason id={reasonId} reason={action.disabledReason} />
    </Tooltip>
  )
}

/**
 * A side-by-side action button that opens a menu of where to point the
 * action. Held back, it keeps its menu shut and says why, as a plain action
 * button does, without changing the elements it is made of.
 */
function InlineTargetsAction({ action }: ActionProps) {
  const reasonId = useId()
  const [tooltipOpen, setTooltipOpen] = useState(false)
  const [menuOpen, setMenuOpen] = useState(false)
  const disabled = action.disabledReason !== undefined
  return (
    <Tooltip open={disabled && tooltipOpen} onOpenChange={setTooltipOpen}>
      <DropdownMenu open={!disabled && menuOpen} onOpenChange={(open) => setMenuOpen(open && !disabled)}>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              aria-disabled={disabled}
              aria-describedby={disabled ? reasonId : undefined}
              className={cn(controlClasses, 'px-2')}
            >
              {action.label}
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <DropdownMenuContent align="end" className="w-48">
          {action.targets?.map((target) => (
            <DropdownMenuItem key={target.id} className="text-[12px]" onSelect={() => action.onSelect(target.id)}>
              {target.label}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
      <HeldBackReason id={reasonId} reason={action.disabledReason} />
    </Tooltip>
  )
}

/**
 * Why a side-by-side action is held back: the tooltip, portalled to the page
 * so the scrolling list can't clip it, and the same words for a screen
 * reader. Nothing while the action can run.
 */
function HeldBackReason({ id, reason }: HeldBackReasonProps) {
  if (reason === undefined) {
    return null
  }
  return (
    <>
      <TooltipContent sideOffset={4} className="px-2 py-1 font-mono text-[11px] leading-[1.4]">
        {reason}
      </TooltipContent>
      <span id={id} className="sr-only">
        {reason}
      </span>
    </>
  )
}

/**
 * An action in the ⋯ menu that opens a submenu of where to point it.
 */
function MenuTargetsAction({ action }: ActionProps) {
  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger className="px-2 py-1.5 text-[12px]">{action.label}</DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="w-48">
        {action.targets?.map((target) => (
          <DropdownMenuItem key={target.id} className="text-[12px]" onSelect={() => action.onSelect(target.id)}>
            {target.label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  )
}

/**
 * An action in the ⋯ menu. A disabled one is greyed out and carries its
 * reason as a second line, since a menu item has no hover tooltip.
 */
function MenuAction({ action }: ActionProps) {
  if (action.disabledReason === undefined && action.targets !== undefined) {
    return <MenuTargetsAction action={action} />
  }
  return (
    <DropdownMenuItem
      disabled={action.disabledReason !== undefined}
      className="flex-col items-stretch gap-0.5 px-2 py-1.5 text-[12px]"
      onSelect={() => action.onSelect()}
    >
      <span>{action.label}</span>
      {action.disabledReason === undefined ? null : (
        <span className="text-[10.5px] text-muted-strong">{action.disabledReason}</span>
      )}
    </DropdownMenuItem>
  )
}
