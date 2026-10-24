import type { ReactElement, ReactNode } from 'react'

import { Tooltip as TooltipPrimitive } from 'radix-ui'

type PortalTooltipProps = {
  /**
   * The hover/focus target. Must be a single element that forwards its ref.
   */
  children: ReactElement
  /**
   * Bubble content.
   */
  label: ReactNode
}

/**
 * The `TooltipBubble` look, rendered through a portal instead of inside the
 * trigger. Use it inside scroll containers: an in-place bubble either widens
 * the container's scrollable area or gets clipped by it, a portalled one
 * does neither, and Radix keeps it inside the window.
 *
 * Carries its own provider so call sites need no setup.
 */
export function PortalTooltip({ children, label }: PortalTooltipProps) {
  return (
    <TooltipPrimitive.Provider delayDuration={0}>
      <TooltipPrimitive.Root>
        <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
        <TooltipPrimitive.Portal>
          <TooltipPrimitive.Content
            side="top"
            sideOffset={6}
            collisionPadding={8}
            className="pointer-events-none z-50 whitespace-nowrap rounded bg-ink px-2 py-1 font-mono text-mono leading-[1.4] text-cream shadow-md"
          >
            {label}
          </TooltipPrimitive.Content>
        </TooltipPrimitive.Portal>
      </TooltipPrimitive.Root>
    </TooltipPrimitive.Provider>
  )
}

export type { PortalTooltipProps }
