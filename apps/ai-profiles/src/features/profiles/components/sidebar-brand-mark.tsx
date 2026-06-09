/**
 * Brand mark — app icon beside the product name.
 * Sized 22×22 to match the prototype's sidebar header.
 */
export function SidebarBrandMark() {
  return (
    <div className="flex items-center gap-2 px-2 pt-0.5 pb-3.5">
      <img src="/icon.svg" alt="" aria-hidden className="size-[22px]" />
      <span className="text-[14.5px] font-semibold tracking-[-0.018em] text-ink">
        ai<span className="font-medium text-muted">-profiles</span>
      </span>
    </div>
  )
}
