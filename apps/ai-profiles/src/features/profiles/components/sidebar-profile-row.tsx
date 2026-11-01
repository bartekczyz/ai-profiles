import type { ReactNode } from 'react'
import type { Surfaces } from '@/lib/types'

import { SidebarSurfaceIcons } from './sidebar-surface-icons'

type Props = {
  name: string
  swatch: ReactNode
  surfaces: Surfaces
  selected: boolean
  /**
   * App mark for the row's leading column, shown only when the sidebar spans
   * more than one app. The column is reserved either way so that every row —
   * default, managed, sortable or not — puts its swatch and name on the same
   * vertical line, and so the sortable variant has a slot for its grip.
   */
  glyph?: ReactNode
  /**
   * Index into the managed list, used for the ⌘N label. Omit for default rows.
   */
  shortcutIndex?: number
  onSelect: () => void
}

/**
 * The row grid. `SortableProfileRow` wraps this very component rather than
 * restating it, so a filtered list — which swaps sortable rows for plain ones
 * — keeps identical columns as you type.
 *
 * `transform-gpu` pins the row on its own GPU layer, which stops the
 * contained icons sub-pixel jittering during the hover/active background
 * transition.
 */
const rowClassName =
  'group/row grid w-full transform-gpu grid-cols-[14px_10px_1fr_auto_auto] items-center gap-2 rounded-md py-[7px] pr-2 pl-2 text-left transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:bg-black/[0.045] dark:hover:bg-white/[0.05] data-[active=true]:bg-black/[0.08] dark:data-[active=true]:bg-white/[0.1]'

export function SidebarProfileRow({ name, swatch, surfaces, selected, glyph, shortcutIndex, onSelect }: Props) {
  const hasShortcut = shortcutIndex !== undefined && shortcutIndex < 9
  return (
    <button
      type="button"
      data-active={selected ? 'true' : 'false'}
      aria-keyshortcuts={hasShortcut ? `Meta+${shortcutIndex + 1}` : undefined}
      onClick={onSelect}
      className={rowClassName}
    >
      {/* The glyph steps aside for the drag grip that `SortableProfileRow`
          parks in this same cell — the two would otherwise overlap. Outside a
          sortable wrapper the group selectors never match, so the glyph just
          stays put. */}
      <span className="inline-grid h-[13px] w-[13px] place-items-center transition-opacity duration-(--duration-snap) ease-(--ease-natural) group-hover/sortable:opacity-0 group-focus-within/sortable:opacity-0">
        {glyph}
      </span>
      {swatch}
      <span className="truncate text-[13px] font-medium tracking-[-0.005em] text-ink">{name}</span>
      <SidebarSurfaceIcons surfaces={surfaces} />
      {hasShortcut ? (
        // Bare label rather than a Kbd chip: at 200px the sidebar cannot spare
        // the chip's padding and border, and profile names are what the column
        // is for. Still `font-kbd`, not `font-mono` — Geist Mono has no ⌘, so
        // mono here would draw the symbol and the digit in different faces.
        <span
          aria-hidden
          className="font-kbd text-[10px] leading-none text-muted-strong transition-opacity opacity-60 group-hover/row:opacity-100 group-data-[active=true]/row:opacity-100"
        >
          ⌘{shortcutIndex + 1}
        </span>
      ) : (
        <span />
      )}
    </button>
  )
}
