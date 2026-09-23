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
   * A second column beside the body once the pane is wide enough, and below
   * it otherwise. Absent, the pane is the single column it always was.
   */
  aside?: ReactNode
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
 *
 * With an `aside`, the pane becomes a size container. Below 1072px of pane
 * width — the default window less the sidebar — the aside simply follows the
 * body and the whole column scrolls as before. From 1072px the body turns
 * into two columns under a header widened to span both: the body column
 * scrolls on its own, and the aside gets the full height to lay out as it
 * likes. The aside is rendered once either way and only CSS moves it, so its
 * state (a half-typed search, say) survives a resize across the breakpoint.
 */
export function PaneLayout({ header, children, aside, className }: Props) {
  const hasAside = aside !== undefined
  return (
    <main className={cn('flex flex-1 flex-col overflow-hidden', hasAside && '@container/pane', className)}>
      <div className="shrink-0 overflow-hidden px-10 pt-10 [scrollbar-gutter:stable]">
        <div className={cn('mx-auto w-full max-w-[640px]', hasAside && '@min-[1072px]/pane:max-w-[1120px]')}>
          {header}
        </div>
      </div>
      <div
        className={cn(
          'flex-1 overflow-y-auto px-10 pt-5 pb-4 [scrollbar-gutter:stable]',
          hasAside && 'min-h-0 @min-[1072px]/pane:overflow-hidden',
        )}
      >
        {hasAside ? (
          <div className="mx-auto w-full max-w-[640px] @min-[1072px]/pane:grid @min-[1072px]/pane:h-full @min-[1072px]/pane:max-w-[1120px] @min-[1072px]/pane:grid-cols-[minmax(0,640px)_minmax(0,1fr)] @min-[1072px]/pane:grid-rows-[minmax(0,1fr)] @min-[1072px]/pane:gap-8">
            <div className="@min-[1072px]/pane:min-h-0 @min-[1072px]/pane:overflow-y-auto">{children}</div>
            <div className="@min-[1072px]/pane:flex @min-[1072px]/pane:min-h-0 @min-[1072px]/pane:flex-col">
              {aside}
            </div>
          </div>
        ) : (
          <div className="mx-auto w-full max-w-[640px]">{children}</div>
        )}
      </div>
    </main>
  )
}
