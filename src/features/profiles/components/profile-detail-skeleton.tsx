import { Skeleton } from '@/design'

/**
 * Structural placeholder for the profile detail pane. Keeps header,
 * surface cards, and timeline at their final positions to avoid layout
 * shift when real data arrives.
 */
export function ProfileDetailSkeleton() {
  return (
    <section className="flex min-w-0 flex-1 flex-col p-10">
      <div className="mb-6 flex items-start gap-4 border-b border-border-soft pb-6">
        <Skeleton className="h-11 w-11 rounded-xl" />
        <div className="flex-1">
          <Skeleton className="mb-2 h-6 w-40" />
          <Skeleton shape="text" className="w-56" />
        </div>
      </div>
      <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
        {[0, 1].map((card) => (
          <div key={card} className="rounded-xl border border-border bg-white p-5 dark:bg-cream-2">
            <Skeleton className="mb-3 h-6 w-24" />
            <Skeleton shape="text" className="mb-2 w-full" />
            <Skeleton shape="text" className="w-3/4" />
            <Skeleton className="mt-4 h-8 w-28" />
          </div>
        ))}
      </div>
    </section>
  )
}
