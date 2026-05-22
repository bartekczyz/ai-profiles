/**
 * Brand mark — orange-gradient tile with a stylized claude-profiles glyph.
 * Sized 22×22 to match the prototype's sidebar header.
 */
export function SidebarBrandMark() {
  return (
    <div className="flex items-center gap-2 px-2 pt-0.5 pb-3.5">
      <span
        aria-hidden
        className="grid h-[22px] w-[22px] place-items-center rounded-md bg-[linear-gradient(160deg,var(--color-orange),var(--color-orange-deep))] shadow-[0_2px_6px_-2px_rgba(217,119,87,0.55),inset_0_1px_0_rgba(255,255,255,0.3)]"
      >
        <svg
          width="12"
          height="12"
          viewBox="0 0 24 24"
          fill="none"
          stroke="white"
          strokeWidth="2.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <title>claude-profiles</title>
          <path d="M12 3 L20 7 v6 c0 4-3.5 7-8 8 -4.5-1-8-4-8-8 V7 z" />
        </svg>
      </span>
      <span className="text-[14.5px] font-semibold tracking-[-0.018em] text-ink">
        claude<span className="font-medium text-muted">-profiles</span>
      </span>
    </div>
  )
}
