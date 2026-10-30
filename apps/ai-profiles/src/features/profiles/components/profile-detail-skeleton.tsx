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
      toolbar={
        <>
          <span className="flex items-center gap-1.5">
            <Skeleton shape="circle" className="size-3.5" />
            <Skeleton shape="text" className="h-3 w-14" />
            <Skeleton shape="text" className="h-3 w-20" />
          </span>
          {/* Edit and the "…" trigger read as one 28px-tall control group. */}
          <Skeleton className="ml-auto h-7 w-[76px] shrink-0 rounded-md" />
        </>
      }
      header={
        <div className="flex items-center gap-4">
          <Skeleton className="h-11 w-11 rounded-xl" />
          <div className="min-w-0 flex-1">
            <Skeleton shape="text" className="h-5 w-36" />
            <Skeleton shape="text" className="mt-2.5 h-3 w-64" />
          </div>
        </div>
      }
    >
      <div className="mb-3 rounded-[10px] border border-border bg-cream/70 px-4 py-3 dark:bg-white/[0.02]">
        <div className="mb-3 flex min-h-[22px] items-center justify-between gap-3.5">
          <Skeleton shape="text" className="h-3 w-12" />
          <Skeleton shape="text" className="h-3 w-20" />
        </div>
        <div className="flex flex-col gap-3">
          {['first', 'second'].map((meter) => (
            <div key={meter} className="grid grid-cols-[140px_1fr_120px] items-center gap-3">
              <Skeleton shape="text" className="h-3 w-24" />
              <Skeleton className="h-1.5 w-full rounded-full" />
              <Skeleton shape="text" className="h-3 w-full" />
            </div>
          ))}
        </div>
      </div>

      {/* The launch panel beside the cost card, as the loaded pane lays them out. */}
      <div className="mb-3 flex items-stretch gap-3">
        <div className="min-w-0 flex-[2] basis-0 overflow-hidden rounded-[10px] border border-border bg-cream/70 dark:bg-white/[0.02]">
          {['desktop', 'cli'].map((surface) => (
            <div
              key={surface}
              className="flex min-h-[52px] items-center justify-between gap-3.5 border-t border-border-soft px-[13px] py-[9px] first:border-t-0"
            >
              <span className="flex min-w-0 items-center gap-[9px]">
                <Skeleton className="h-6 w-6 shrink-0 rounded-[7px]" />
                <span className="min-w-0">
                  <Skeleton shape="text" className="h-3 w-24" />
                  <Skeleton shape="text" className="mt-2 h-2.5 w-40" />
                </span>
              </span>
              <Skeleton className="h-7 w-24 shrink-0 rounded-md" />
            </div>
          ))}
        </div>
        <div className="flex min-w-0 flex-1 basis-0 flex-col justify-between gap-3 rounded-[10px] border border-border bg-cream/70 px-4 py-3 dark:bg-white/[0.02]">
          <Skeleton shape="text" className="h-3 w-32" />
          <Skeleton shape="text" className="h-5 w-20" />
        </div>
      </div>
    </PaneLayout>
  )
}
