import type { ReactNode } from 'react'
import type { Surfaces } from '@/lib/types'

import { Kbd } from '@/design'

import { SidebarSurfaceIcons } from './sidebar-surface-icons'

type Props = {
  name: string
  swatch: ReactNode
  surfaces: Surfaces
  selected: boolean
  /** Index into the managed list, used for the ⌘N chip. Omit for default rows. */
  shortcutIndex?: number
  onSelect: () => void
}

export function SidebarProfileRow({ name, swatch, surfaces, selected, shortcutIndex, onSelect }: Props) {
  const hasShortcut = shortcutIndex !== undefined && shortcutIndex < 9
  return (
    <button
      type="button"
      data-active={selected ? 'true' : 'false'}
      aria-keyshortcuts={hasShortcut ? `Meta+${shortcutIndex + 1}` : undefined}
      onClick={onSelect}
      className="group grid w-full transform-gpu grid-cols-[14px_1fr_auto_auto] items-center gap-2.5 rounded-md py-[7px] pr-2.5 pl-3 text-left cursor-pointer transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:bg-white/45 dark:hover:bg-white/[0.04] data-[active=true]:bg-white/72 data-[active=true]:shadow-[0_1px_2px_rgba(40,30,20,0.04),inset_0_0_0_1px_rgba(229,224,210,0.55)] dark:data-[active=true]:bg-white/[0.08] dark:data-[active=true]:shadow-[0_1px_2px_rgba(0,0,0,0.2),inset_0_0_0_1px_rgba(255,255,255,0.05)]"
    >
      {swatch}
      <span className="truncate text-[13px] font-medium tracking-[-0.005em] text-ink">{name}</span>
      <SidebarSurfaceIcons surfaces={surfaces} />
      {hasShortcut ? (
        <span className="inline-flex items-center transition-opacity opacity-40 group-hover:opacity-100 group-data-[active=true]:opacity-100">
          <Kbd>⌘{shortcutIndex + 1}</Kbd>
        </span>
      ) : (
        <span />
      )}
    </button>
  )
}
