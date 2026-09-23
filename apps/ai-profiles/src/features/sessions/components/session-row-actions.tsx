import { useId } from 'react'

import { MoreHorizontal } from 'lucide-react'

import { cn } from '@/design'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/design/ui/dropdown-menu'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/design/ui/tooltip'

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
   * Runs the action.
   */
  onSelect: () => void
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

const controlClasses =
  'inline-flex h-7 shrink-0 cursor-pointer items-center rounded-[7px] text-[12px] text-muted outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:bg-ink/[0.06] hover:text-ink focus-visible:ring-2 focus-visible:ring-orange/40 aria-disabled:cursor-default aria-disabled:opacity-50 aria-disabled:hover:bg-transparent aria-disabled:hover:text-muted'

/**
 * A row's actions, laid out for the room the panel has: side by side as
 * buttons once the panel is at least 480px wide, folded into a ⋯ menu below
 * that. Both are rendered and a container query on the panel shows one, so
 * the switch needs no measuring.
 */
export function SessionRowActions({ actions }: Props) {
  return (
    <>
      <div className="hidden shrink-0 items-center gap-0.5 @min-[480px]/sessions:flex">
        {actions.map((action) => (
          <InlineAction key={action.id} action={action} />
        ))}
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
 * tooltip is portalled to the page, so the scrolling list can't clip it.
 */
function InlineAction({ action }: ActionProps) {
  const reasonId = useId()
  const disabled = action.disabledReason !== undefined
  const button = (
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
  )
  if (!disabled) {
    return button
  }
  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>{button}</TooltipTrigger>
        <TooltipContent sideOffset={4} className="px-2 py-1 font-mono text-[11px] leading-[1.4]">
          {action.disabledReason}
        </TooltipContent>
      </Tooltip>
      <span id={reasonId} className="sr-only">
        {action.disabledReason}
      </span>
    </TooltipProvider>
  )
}

/**
 * An action in the ⋯ menu. A disabled one is greyed out and carries its
 * reason as a second line, since a menu item has no hover tooltip.
 */
function MenuAction({ action }: ActionProps) {
  return (
    <DropdownMenuItem
      disabled={action.disabledReason !== undefined}
      className="flex-col items-stretch gap-0.5 px-2 py-1.5 text-[12px]"
      onSelect={action.onSelect}
    >
      <span>{action.label}</span>
      {action.disabledReason === undefined ? null : (
        <span className="text-[10.5px] text-muted-strong">{action.disabledReason}</span>
      )}
    </DropdownMenuItem>
  )
}
