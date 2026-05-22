import { Skeleton } from '@/design'

/**
 * Four placeholder rows mirroring the sidebar layout — keeps the panel
 * width and row heights stable while profiles load.
 */
export function SidebarSkeleton() {
  return (
    <aside className="flex w-64 flex-col gap-2 border-r border-border bg-cream-2 p-3">
      <Skeleton className="h-6 w-32" />
      <Skeleton className="h-8 w-full" />
      <div className="mt-3 flex flex-col gap-1">
        {[0, 1, 2, 3].map((row) => (
          <div key={row} className="flex items-center gap-2 px-2 py-1.5">
            <Skeleton shape="circle" className="h-2.5 w-2.5" />
            <Skeleton shape="text" className="w-32" />
          </div>
        ))}
      </div>
    </aside>
  )
}
