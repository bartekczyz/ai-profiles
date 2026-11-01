import type { ReactNode } from 'react'
import type { Surfaces } from '@/lib/types'

import { type AnimateLayoutChanges, useSortable } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { DotsSixVertical } from '@phosphor-icons/react'

import { SidebarProfileRow } from './sidebar-profile-row'

type Props = {
  name: string
  swatch: ReactNode
  surfaces: Surfaces
  selected: boolean
  sortableId: string
  glyph?: ReactNode
  shortcutIndex?: number
  onSelect: () => void
}

/**
 * Sortable wrapper around the sidebar row.
 *
 * Renders the plain `SidebarProfileRow` unchanged — so the two variants stay
 * pixel-identical — and adds a grip handle (Lucide `grip-vertical`) that owns
 * the drag listeners. The grip sits in the row's leading column, on top of the
 * app glyph, which fades out while the grip is showing; a dedicated gutter
 * would cost the width the profile names need at 200px.
 *
 * The row itself stays a click target — dnd-kit's pointer sensor uses an
 * activation-distance threshold so a normal click never accidentally starts a
 * drag.
 *
 * Keyboard reorder lands via dnd-kit's KeyboardSensor (Space to grab, arrows
 * to move, Space/Enter to drop, Esc to cancel) — the grip button is the focus
 * target, and focusing anywhere in the row reveals it.
 */
// Animate the *during-drag* shuffle (siblings sliding to make room) but
// skip the *post-drop* layout-change animation. Our optimistic cache
// update reorders the data in the same tick the drop fires, and dnd-kit's
// default would then animate each item from its pre-reorder slot to its
// new slot — which makes the row that ends up at the top look like it
// slides in from above the list. Letting the DOM snap into place after
// drop avoids that artifact; the drag transform itself smooths the move.
const animateLayoutChanges: AnimateLayoutChanges = (args) => args.isSorting

export function SortableProfileRow({
  name,
  swatch,
  surfaces,
  selected,
  sortableId,
  glyph,
  shortcutIndex,
  onSelect,
}: Props) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: sortableId,
    animateLayoutChanges,
  })

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  }

  return (
    <div
      ref={setNodeRef}
      style={style}
      data-dragging={isDragging ? 'true' : 'false'}
      className="group/sortable relative data-[dragging=true]:z-10 data-[dragging=true]:scale-[1.02] data-[dragging=true]:shadow-card-hover"
    >
      <SidebarProfileRow
        name={name}
        swatch={swatch}
        surfaces={surfaces}
        selected={selected}
        glyph={glyph}
        shortcutIndex={shortcutIndex}
        onSelect={onSelect}
      />
      {/* Grip — absolutely placed over the row's leading column. Owns the drag
          listeners so dragging it doesn't fire the button click.

          Hidden at rest, unlike the pre-redesign grip which sat at low opacity
          so it could be discovered without hovering. It has to be: this cell
          now holds the row's app glyph, which fades out exactly as the grip
          fades in, and showing both at rest would stack them. The alternative
          — a dedicated gutter for the grip — costs width the profile names
          need at 200px. Discoverability is traded for the glyph deliberately;
          keyboard reorder is unaffected, since focus reveals the grip. */}
      <button
        type="button"
        {...attributes}
        {...listeners}
        // aria-label must come AFTER the dnd-kit attribute spread so our
        // per-row label ("Reorder Personal") wins over the generic
        // role/aria-roledescription dnd-kit attaches to every sortable.
        aria-label={`Reorder ${name}`}
        className="absolute top-1/2 left-1.5 grid h-6 w-4 -translate-y-1/2 cursor-grab place-items-center text-muted-strong outline-none opacity-0 transition-opacity duration-(--duration-snap) ease-(--ease-natural) group-hover/sortable:opacity-100 group-focus-within/sortable:opacity-100 active:cursor-grabbing"
      >
        <DotsSixVertical className="h-3.5 w-3.5" />
      </button>
    </div>
  )
}
