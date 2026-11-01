import { PaneLayout } from '@/components/pane-layout'
import { Skeleton } from '@/design'

/**
 * Skeleton for the Settings pane — the same pinned header over three stacked
 * sections matching the Appearance / System / Data trio.
 */
export function SettingsViewSkeleton() {
  return (
    <PaneLayout
      header={
        <div className="flex items-center justify-between">
          <Skeleton shape="text" className="h-5 w-28" />
          <Skeleton className="h-7 w-16 rounded-md" />
        </div>
      }
    >
      {['appearance', 'system', 'data'].map((section) => (
        <div key={section} className="mb-8">
          <Skeleton shape="text" className="mb-3 h-3 w-24" />
          <Skeleton className="h-24 w-full rounded-[10px]" />
        </div>
      ))}
    </PaneLayout>
  )
}
