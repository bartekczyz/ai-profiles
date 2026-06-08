import { Skeleton } from '@/design'

/**
 * Skeleton for the Settings pane — three stacked sections matching the
 * Appearance / System / Data trio that lands later in the redesign.
 */
export function SettingsViewSkeleton() {
  return (
    <section className="flex min-w-0 flex-1 flex-col p-10">
      <Skeleton className="mb-4 h-7 w-32" />
      {[0, 1, 2].map((section) => (
        <div key={section} className="mb-8">
          <Skeleton shape="text" className="mb-3 w-24" />
          <Skeleton className="h-24 w-full rounded-xl" />
        </div>
      ))}
    </section>
  )
}
