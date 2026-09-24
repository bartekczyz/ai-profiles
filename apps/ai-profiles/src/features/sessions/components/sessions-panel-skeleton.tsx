import { Skeleton } from '@/design'

/**
 * The panel's outer box, shared by the panel and its skeleton so both sit in
 * the pane the same way: a flex column that fills the height the pane gives
 * it, beside the details or under them.
 */
export const sessionsPanelClasses = '@container/sessions mb-6 flex min-h-0 flex-1 flex-col gap-2.5'

/**
 * The same inset grouped panel as the surfaces block. It takes the panel's
 * height left under the tabs and controls, and its rows scroll inside it.
 *
 * It positions its rows' absolute bits (a held-back action's screen-reader
 * text), so they scroll and clip with the rows. Otherwise they'd resolve
 * against the app root, outside every scroller, and a row far down the list
 * would stretch the window's document and let the whole app scroll.
 */
export const sessionsListClasses =
  'relative min-h-0 overflow-y-auto rounded-[10px] border border-border bg-white/50 [scrollbar-gutter:stable] dark:bg-white/[0.035]'

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
 * tabs, the controls row and the list, each at its real height, so the pane
 * takes its final shape — two columns included — from the first paint.
 */
export function SessionsPanelSkeleton() {
  return (
    <div aria-hidden className={sessionsPanelClasses}>
      <div className="flex h-8 items-center gap-4">
        <Skeleton shape="text" className="h-3 w-16" />
        <Skeleton shape="text" className="h-3 w-20" />
      </div>
      <div className="flex items-center gap-2">
        <Skeleton className="h-9 min-w-40 flex-1 rounded-md" />
        <Skeleton className="h-9 w-24 shrink-0 rounded-md" />
      </div>
      <SessionsListSkeleton />
    </div>
  )
}
