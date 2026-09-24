import { Skeleton } from '@/design'

/**
 * The panel's outer box, shared by the panel and its skeleton so both sit in
 * the pane the same way: a card in the Usage card's style, and a flex column
 * that fills the height the pane gives it, beside the details or under them.
 */
export const sessionsPanelClasses =
  '@container/sessions mb-6 flex min-h-0 flex-1 flex-col gap-0 overflow-clip rounded-[10px] border border-border-soft bg-white/30 dark:bg-white/[0.02]'

/**
 * The card's eyebrow row: the SESSIONS label, with the tabs across from it.
 */
export const sessionsHeaderClasses =
  'mx-[13px] mt-[9px] flex min-h-[22px] shrink-0 items-center justify-between gap-3.5'

/**
 * The rows' box: the rest of the card under a hairline. Its rows scroll
 * inside it.
 *
 * It positions its rows' absolute bits (a held-back action's screen-reader
 * text), so they scroll and clip with the rows. Otherwise they'd resolve
 * against the app root, outside every scroller, and a row far down the list
 * would stretch the window's document and let the whole app scroll.
 */
export const sessionsListClasses =
  'relative min-h-0 flex-1 overflow-y-auto border-t border-border-soft [scrollbar-gutter:stable]'

/**
 * Stable keys for the placeholder rows, one per row.
 */
const skeletonRowKeys = ['first', 'second', 'third']

/**
 * Three placeholder rows shaped like the real ones, in the real list's box.
 */
export function SessionsListSkeleton() {
  return (
    <div aria-hidden className={sessionsListClasses}>
      {skeletonRowKeys.map((key) => (
        <div key={key} className="border-t border-border-soft px-[13px] py-[11px] first:border-t-0">
          <Skeleton shape="text" className="h-3 w-48 max-w-full" />
          <Skeleton shape="text" className="mt-2 h-2.5 w-32 max-w-full" />
        </div>
      ))}
    </div>
  )
}

/**
 * Stand-in for the whole panel while the pane itself is still loading: the
 * header, the toolbar and the list, each at its real height, so the pane
 * takes its final shape — two columns included — from the first paint.
 */
export function SessionsPanelSkeleton() {
  return (
    <div aria-hidden className={sessionsPanelClasses}>
      <div className={sessionsHeaderClasses}>
        <Skeleton shape="text" className="h-2.5 w-16" />
        <Skeleton className="h-6 w-32 rounded-lg" />
      </div>
      <div className="flex items-center gap-1.5 px-[13px] pt-2 pb-[9px]">
        <Skeleton className="h-7 min-w-0 flex-1 rounded-[7px]" />
        <Skeleton className="h-7 w-7 shrink-0 rounded-[7px]" />
      </div>
      <SessionsListSkeleton />
    </div>
  )
}
