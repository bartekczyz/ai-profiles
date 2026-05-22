import type { Profile } from '@/lib/types'

import { Kbd } from '@/design'

import { SidebarSurfaceIcons } from './sidebar-surface-icons'

type Props = {
  profile: Profile
  index: number
  selected: boolean
  onSelect: () => void
}

/**
 * One row in the sidebar profile list. Active state is driven by `selected`
 * — the parent owns selection state. The Mod+{index+1} kbd chip is decorative
 * until Phase 11 hooks it into the shortcut registry.
 */
export function SidebarProfileRow({ profile, index, selected, onSelect }: Props) {
  return (
    <button
      type="button"
      data-active={selected ? 'true' : 'false'}
      onClick={onSelect}
      aria-keyshortcuts={index < 9 ? `Meta+${index + 1}` : undefined}
      // `transform-gpu` (= translate3d(0,0,0)) promotes the row to its own
      // GPU layer so the surface icons inside don't sub-pixel jitter when
      // the row's semi-transparent hover/active background is composited
      // over the sidebar cream.
      className="group grid w-full transform-gpu grid-cols-[14px_1fr_auto_auto] items-center gap-2.5 rounded-md py-[7px] pr-2.5 pl-3 text-left cursor-pointer transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:bg-white/45 dark:hover:bg-white/[0.04] data-[active=true]:bg-white/72 data-[active=true]:shadow-[0_1px_2px_rgba(40,30,20,0.04),inset_0_0_0_1px_rgba(229,224,210,0.55)] dark:data-[active=true]:bg-white/[0.08] dark:data-[active=true]:shadow-[0_1px_2px_rgba(0,0,0,0.2),inset_0_0_0_1px_rgba(255,255,255,0.05)]"
    >
      <span
        aria-hidden
        className="inline-block h-2.5 w-2.5 rounded-full shadow-[inset_0_0_0_1px_rgba(0,0,0,0.08),0_1px_1px_rgba(0,0,0,0.06)]"
        style={{ background: profile.color }}
      />
      <span className="truncate text-[13px] font-medium tracking-[-0.005em] text-ink">{profile.name}</span>
      <SidebarSurfaceIcons surfaces={profile.surfaces} />
      {index < 9 ? (
        // `inline-flex items-center` forces the chip to be vertically
        // centred inside its grid cell. The plain `<span>` wrapper we had
        // before relied on the default inline-baseline alignment, which
        // can leave the chip a pixel off when the row contains items of
        // mixed heights (12px icons + 18px chip).
        <span className="inline-flex items-center transition-opacity opacity-40 group-hover:opacity-100 group-data-[active=true]:opacity-100">
          <Kbd>⌘{index + 1}</Kbd>
        </span>
      ) : null}
    </button>
  )
}
