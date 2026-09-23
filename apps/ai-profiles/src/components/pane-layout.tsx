import type { ReactNode } from 'react'

import { cn } from '@/design'

type Props = {
  /**
   * Pinned above the scroll region — it never scrolls away.
   */
  header: ReactNode
  /**
   * The pane body; the only part that scrolls.
   */
  children: ReactNode
  /**
   * Extra classes for the pane root — e.g. an opaque background over the
   * window gradient.
   */
  className?: string
}

/**
 * Right-pane chrome: a pinned header over one scrollable content column.
 *
 * The header sits outside the scroll container rather than being
 * `position: sticky` inside it, so it needs no opaque fill to hide content
 * scrolling beneath it — the pane background shows through, and content
 * clips at the header's bottom edge instead.
 *
 * Both rows reserve the scrollbar gutter, so the header column stays aligned
 * with the body column when a classic (non-overlay) scrollbar appears.
 *
 * The bottom padding is deliberately smaller than the top: the last block a
 * pane renders carries its own 24px bottom margin, so 16px here lands the
 * same gutter as the 40px above.
 */
export function PaneLayout({ header, children, className }: Props) {
  return (
    <main className={cn('flex flex-1 flex-col overflow-hidden', className)}>
      <div className="shrink-0 overflow-hidden px-10 pt-10 [scrollbar-gutter:stable]">
        <div className="mx-auto w-full max-w-[640px]">{header}</div>
      </div>
      <div className="flex-1 overflow-y-auto px-10 pt-5 pb-4 [scrollbar-gutter:stable]">
        <div className="mx-auto w-full max-w-[640px]">{children}</div>
      </div>
    </main>
  )
}
