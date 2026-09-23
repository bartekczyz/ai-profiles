import { PaneLayout } from '@/components/pane-layout'
import { Skeleton } from '@/design'
import { SessionsPanelSkeleton } from '@/features/sessions/components/sessions-panel-skeleton'

/**
 * Cold-load placeholder for the profile detail pane. It mirrors the pane's
 * real chrome rather than approximating it: the same `PaneLayout`, the same
 * header rule, both inset panels drawn with their own borders and
 * paddings, and the sessions panel's placeholder as the aside — so at a
 * two-column width the skeleton is already two columns. Only the content inside them is a placeholder, so
 * resolving the profile swaps text in place instead of moving the panels.
 *
 * Only the very first app boot reaches this. Once a profile is selected the
 * header renders from sidebar-provided data immediately, and the usage and
 * surfaces panels each carry their own granular fallback.
 */
export function ProfileDetailSkeleton() {
  return (
    <PaneLayout
      aside={<SessionsPanelSkeleton />}
      header={
        <div className="flex items-center gap-3.5 border-b border-border-soft pb-5">
          <Skeleton className="h-11 w-11 rounded-xl" />
          <div className="min-w-0 flex-1">
            <Skeleton className="h-[27px] w-40 rounded-sm" />
            <Skeleton shape="text" className="mt-1 h-3.5 w-44" />
          </div>
          {/* Edit and the "…" trigger read as one 26px-tall control group. */}
          <Skeleton className="h-[26px] w-[72px] shrink-0 rounded-md" />
        </div>
      }
    >
      <div className="mb-3.5 rounded-[10px] border border-border-soft bg-white/30 px-[13px] py-[9px] dark:bg-white/[0.02]">
        <div className="mb-1.5 flex min-h-[22px] items-center justify-between gap-3.5">
          <Skeleton shape="text" className="h-2.5 w-12" />
          <Skeleton shape="text" className="h-2.5 w-20" />
        </div>
        <div className="flex flex-col gap-2">
          {[0, 1, 2].map((meter) => (
            <div
              key={meter}
              className="grid min-h-[15px] grid-cols-[32px_1fr_180px] items-center gap-2 lg:grid-cols-[140px_1fr_180px] lg:gap-3"
            >
              <Skeleton shape="text" className="h-2.5 w-full" />
              <Skeleton className="h-1.5 w-full rounded-full" />
              <Skeleton shape="text" className="h-2.5 w-full" />
            </div>
          ))}
        </div>
      </div>

      <div className="mb-6 overflow-hidden rounded-[10px] border border-border bg-white/50 dark:bg-white/[0.035]">
        {[0, 1].map((surface) => (
          <div
            key={surface}
            className="flex min-h-[42px] items-center justify-between gap-3.5 border-t border-border-soft px-[13px] py-[9px] first:border-t-0"
          >
            <span className="flex min-w-0 items-center gap-[9px]">
              <Skeleton className="h-6 w-6 shrink-0 rounded-[7px]" />
              <span className="min-w-0">
                <Skeleton shape="text" className="h-3 w-24" />
                <Skeleton shape="text" className="mt-1.5 h-2.5 w-44" />
              </span>
            </span>
            <Skeleton className="h-7 w-28 shrink-0 rounded-[7px]" />
          </div>
        ))}
      </div>
    </PaneLayout>
  )
}
