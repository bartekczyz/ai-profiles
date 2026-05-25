import { BrandSparkle } from './brand-sparkle'

/**
 * Brand mark — orange-gradient tile with the claude-profiles sparkle.
 * Sized 22×22 to match the prototype's sidebar header.
 */
export function SidebarBrandMark() {
  return (
    <div className="flex items-center gap-2 px-2 pt-0.5 pb-3.5">
      <span
        aria-hidden
        className="grid h-[22px] w-[22px] place-items-center rounded-md bg-[linear-gradient(160deg,var(--color-orange),var(--color-orange-deep))] text-white shadow-[0_2px_6px_-2px_rgba(217,119,87,0.55),inset_0_1px_0_rgba(255,255,255,0.3)]"
      >
        <BrandSparkle size={14} />
      </span>
      <span className="text-[14.5px] font-semibold tracking-[-0.018em] text-ink">
        claude<span className="font-medium text-muted">-profiles</span>
      </span>
    </div>
  )
}
