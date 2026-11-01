import type { ReactNode } from 'react'

import { useRef } from 'react'

import { cn } from '@/design'
import { useWindowDragRegion } from '@/lib/use-window-drag-region'

/**
 * The rounded panel the right-hand panes sit on, inset from the window so the
 * sidebar material shows in the gutter around it. Shared with the boot
 * skeleton so the panel doesn't jump when the app finishes loading.
 */
export const insetPanelClasses =
  'my-2 mr-2 flex min-w-0 flex-1 overflow-hidden rounded-[10px] bg-base shadow-[0_0_0_1px_var(--color-border),0_1px_3px_rgba(0,0,0,0.06)]'

type Props = {
  /**
   * The top of the content column. It scrolls away with the page.
   */
  header: ReactNode
  /**
   * The pane body.
   */
  children: ReactNode
  /**
   * A bar pinned to the top of the pane, divided from the content under it:
   * where you are, and the pane's actions. It doubles as a window drag area.
   */
  toolbar?: ReactNode
  /**
   * Follows the body in the same column and takes the height left under it,
   * so a long list scrolls inside it rather than stretching the page. It keeps
   * a floor of its own; once the pane is too short for that, the column scrolls.
   */
  aside?: ReactNode
  /**
   * Extra classes for the pane root.
   */
  className?: string
}

/**
 * Right-pane chrome: an optional pinned toolbar over one scrollable column
 * holding the header, the body and the aside.
 *
 * A pane with an aside is a wide reading column; one without (Settings)
 * stays narrower, since its rows are forms rather than data. The scroll
 * region reserves its scrollbar gutter, so the column doesn't shift when a
 * classic scrollbar appears.
 */
export function PaneLayout({ toolbar, header, children, aside, className }: Props) {
  const hasAside = aside !== undefined
  // With an aside, the column is at least as tall as the scroll region, and
  // grows past it only when the content under its zero-basis aside needs to.
  const column = cn('mx-auto w-full', hasAside ? 'flex max-w-[1040px] shrink-0 grow flex-col' : 'max-w-[720px]')
  const toolbarRef = useRef<HTMLDivElement>(null)
  useWindowDragRegion(toolbarRef)
  return (
    <main className={cn('flex flex-1 flex-col overflow-hidden', className)}>
      {toolbar ? (
        <div
          ref={toolbarRef}
          className="relative z-20 flex h-11 shrink-0 items-center gap-2 border-b border-border pr-2.5 pl-4"
        >
          {toolbar}
        </div>
      ) : null}
      <div
        className={cn(
          'flex-1 overflow-y-auto px-10 pb-4 [scrollbar-gutter:stable]',
          toolbar ? 'pt-8' : 'pt-10',
          hasAside && 'flex min-h-0 flex-col',
        )}
      >
        <div className={column}>
          <div className="mb-6">{header}</div>
          {children}
          {hasAside ? <div className="flex min-h-80 grow basis-0 flex-col">{aside}</div> : null}
        </div>
      </div>
    </main>
  )
}
