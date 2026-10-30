import type { Surfaces } from '@/lib/types'

import { Monitor, TerminalWindow } from '@phosphor-icons/react'

import { cn } from '@/design'

type Props = {
  surfaces: Surfaces
}

/**
 * The two surface glyphs (desktop monitor + terminal) sit at the right edge
 * of a profile row. Surfaces toggled off render at 22% opacity so the row
 * still reads as a single profile but the missing capability is obvious.
 */
export function SidebarSurfaceIcons({ surfaces }: Props) {
  return (
    <span className="flex items-center gap-1 text-muted">
      <Monitor
        aria-label={surfaces.gui ? 'Desktop app enabled' : 'Desktop app disabled'}
        className={cn('h-3 w-3', surfaces.gui ? 'opacity-100' : 'opacity-[0.22]')}
      />
      <TerminalWindow
        aria-label={surfaces.cli ? 'CLI enabled' : 'CLI disabled'}
        className={cn('h-3 w-3', surfaces.cli ? 'opacity-100' : 'opacity-[0.22]')}
      />
    </span>
  )
}
