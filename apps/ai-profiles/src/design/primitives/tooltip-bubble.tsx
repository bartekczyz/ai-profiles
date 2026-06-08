import type { ReactNode } from 'react'

type TooltipBubbleProps = {
  children: ReactNode
}

/**
 * Hover-revealed popup bubble. Dark pill positioned above its closest
 * positioned ancestor, fades in when an ancestor with the `group` class
 * is hovered.
 *
 * The caller owns the anchor — wrap the trigger element in a span/div
 * with `group relative` (or any layout that establishes a containing
 * block). The bubble has `pointer-events-none` so it never interferes
 * with the trigger's own interactions.
 *
 * Typography is fixed by design: every tooltip bubble must render at
 * the same text size regardless of the trigger's ambient font scale.
 * That's why there is no className/style escape hatch — the primitive
 * is intentionally closed.
 *
 * Note: the className is a single plain string. We deliberately do NOT
 * pipe it through `cn` / `tailwind-merge` here. `tailwind-merge` reads
 * `text-mono` (our custom font-size token) and `text-cream` (a colour)
 * as members of the same `text-*` conflict group and silently drops
 * the first one — leaving the bubble without a font-size and rendering
 * at the inherited ambient size, which varies per call site. Keeping
 * the className inert preserves both classes.
 */
export function TooltipBubble({ children }: TooltipBubbleProps) {
  return (
    <span
      role="tooltip"
      className="pointer-events-none absolute bottom-full left-1/2 z-10 mb-1.5 -translate-x-1/2 whitespace-nowrap rounded bg-ink px-2 py-1 font-mono text-mono leading-[1.4] text-cream opacity-0 shadow-md transition-opacity group-hover:opacity-100"
    >
      {children}
    </span>
  )
}

export type { TooltipBubbleProps }
