import { PaneLayout } from '@/components/pane-layout'
import { Skeleton } from '@/design'

/**
 * Skeleton for the Settings pane — the same pinned header over three stacked
 * sections matching the Appearance / System / Data trio.
 */
export function SettingsViewSkeleton() {
  return (
    <PaneLayout
      className="bg-background"
      header={
        <div className="flex items-center justify-between border-b border-border-soft pb-6">
          <Skeleton className="h-7 w-32" />
          <Skeleton className="h-7 w-20 rounded-md" />
        </div>
      }
    >
      {[0, 1, 2].map((section) => (
        <div key={section} className="mb-8">
          <Skeleton shape="text" className="mb-3 w-24" />
          <Skeleton className="h-24 w-full rounded-xl" />
        </div>
      ))}
    </PaneLayout>
  )
}
