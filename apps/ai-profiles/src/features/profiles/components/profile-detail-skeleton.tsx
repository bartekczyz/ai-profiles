import { Skeleton } from '@/design'

/**
 * Cold-load placeholder for the profile detail pane. Matches the real
 * layout (px-10, max-w-640 content column, header with swatch + title +
 * slug + actions, two surface cards in a grid, recent-activity block,
 * danger link, hint strip) so the transition to the loaded view doesn't
 * shift content around.
 *
 * Only the very first app boot uses this — once any profile is selected
 * the header + danger + hint strip render with sidebar-provided data
 * immediately and the surface cards have their own granular fallback.
 */
export function ProfileDetailSkeleton() {
  return (
    <main className="flex flex-1 flex-col overflow-y-auto px-10 pt-10 pb-0">
      <div className="mx-auto w-full max-w-[640px] flex-1">
        <div className="mb-5 flex items-start gap-4 border-b border-border-soft pb-5">
          <Skeleton className="h-11 w-11 rounded-xl" />
          <div className="min-w-0 flex-1 pt-px">
            <Skeleton className="mb-2 h-6 w-40" />
            <Skeleton shape="text" className="h-3 w-56" />
          </div>
          <div className="flex gap-1 pt-1">
            <Skeleton className="h-7 w-14 rounded-md" />
            <Skeleton className="h-7 w-7 rounded-sm" />
          </div>
        </div>

        <div className="mb-6 grid grid-cols-1 gap-3.5 lg:grid-cols-2">
          {[0, 1].map((card) => (
            <div key={card} className="rounded-xl border border-border bg-white p-5 dark:bg-cream-2">
              <div className="mb-2 flex items-center gap-2.5">
                <Skeleton className="h-[26px] w-[26px] rounded-[7px]" />
                <Skeleton shape="text" className="h-3.5 w-28" />
              </div>
              <Skeleton shape="text" className="mb-2 h-3 w-full" />
              <Skeleton shape="text" className="mb-3 h-3 w-3/4" />
              <Skeleton shape="text" className="mb-4 h-2.5 w-44" />
              <div className="flex gap-1.5">
                <Skeleton className="h-7 w-28 rounded-md" />
                <Skeleton className="h-7 w-20 rounded-md" />
                <Skeleton className="h-7 w-20 rounded-md" />
              </div>
            </div>
          ))}
        </div>

        <div className="mb-6">
          <Skeleton shape="text" className="mb-2.5 h-2.5 w-24" />
          <Skeleton shape="text" className="h-3 w-32" />
        </div>

        <Skeleton shape="text" className="h-3 w-72" />
      </div>
      <div className="-mx-10 mt-6 flex justify-center gap-4 border-t border-border-soft px-6 py-3">
        <Skeleton shape="text" className="h-3 w-80" />
      </div>
    </main>
  )
}
