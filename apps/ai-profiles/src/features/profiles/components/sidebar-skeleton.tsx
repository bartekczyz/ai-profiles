import { Skeleton } from '@/design'

/**
 * Four placeholder rows mirroring the sidebar layout — keeps the panel
 * width and row heights stable while profiles load.
 */
export function SidebarSkeleton() {
  return (
    <aside className="flex w-[232px] shrink-0 flex-col px-2.5 pt-11 pb-2.5">
      <div className="flex items-center gap-2 px-2 pt-0.5 pb-3.5">
        <Skeleton className="size-[22px] rounded-md" />
        <Skeleton shape="text" className="h-3.5 w-24" />
      </div>
      <Skeleton className="mb-2.5 h-7 w-full rounded-md" />
      <div className="flex flex-col gap-px">
        {['first', 'second', 'third'].map((row) => (
          <div key={row} className="flex items-center gap-2 px-2 py-[9px]">
            <Skeleton shape="circle" className="size-3.5" />
            <Skeleton shape="circle" className="size-2.5" />
            <Skeleton shape="text" className="h-3 w-24" />
          </div>
        ))}
      </div>
    </aside>
  )
}
