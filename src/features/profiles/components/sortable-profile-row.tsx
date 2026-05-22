import type { Profile } from '@/lib/types'

import { type AnimateLayoutChanges, useSortable } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { GripVertical } from 'lucide-react'

import { cn, Kbd } from '@/design'

import { SidebarSurfaceIcons } from './sidebar-surface-icons'

type Props = {
  profile: Profile
  index: number
  selected: boolean
  onSelect: () => void
}

/**
 * Sortable wrapper around the sidebar row.
 *
 * Replaces SidebarProfileRow inside the DndContext: the row itself is
 * still a button that selects the profile on click, but it also exposes
 * a grip handle (Lucide `grip-vertical`) on the left edge that owns the
 * drag listeners. The button stays a click target — dnd-kit's pointer
 * sensor uses an activation-distance threshold so a normal click never
 * accidentally starts a drag.
 *
 * Keyboard reorder lands via dnd-kit's KeyboardSensor (Space to grab,
 * arrows to move, Space/Enter to drop, Esc to cancel) — the grip button
 * is the focus target.
 */
// Animate the *during-drag* shuffle (siblings sliding to make room) but
// skip the *post-drop* layout-change animation. Our optimistic cache
// update reorders the data in the same tick the drop fires, and dnd-kit's
// default would then animate each item from its pre-reorder slot to its
// new slot — which makes the row that ends up at the top look like it
// slides in from above the list. Letting the DOM snap into place after
// drop avoids that artifact; the drag transform itself smooths the move.
const animateLayoutChanges: AnimateLayoutChanges = (args) => args.isSorting

export function SortableProfileRow({ profile, index, selected, onSelect }: Props) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: profile.id,
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
      <button
        type="button"
        data-active={selected ? 'true' : 'false'}
        onClick={onSelect}
        aria-keyshortcuts={index < 9 ? `Meta+${index + 1}` : undefined}
        // See sibling `sidebar-profile-row` for the `transform-gpu` rationale:
        // pins the row on its own GPU layer so contained icons don't
        // sub-pixel jitter during the hover/active background transition.
        className="group/row grid w-full transform-gpu grid-cols-[14px_1fr_auto_auto] items-center gap-2.5 rounded-md py-[7px] pr-2.5 pl-[22px] text-left cursor-pointer transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:bg-white/45 dark:hover:bg-white/[0.04] data-[active=true]:bg-white/72 data-[active=true]:shadow-[0_1px_2px_rgba(40,30,20,0.04),inset_0_0_0_1px_rgba(229,224,210,0.55)] dark:data-[active=true]:bg-white/[0.08] dark:data-[active=true]:shadow-[0_1px_2px_rgba(0,0,0,0.2),inset_0_0_0_1px_rgba(255,255,255,0.05)]"
      >
        <span
          aria-hidden
          className="inline-block h-2.5 w-2.5 rounded-full shadow-[inset_0_0_0_1px_rgba(0,0,0,0.08),0_1px_1px_rgba(0,0,0,0.06)]"
          style={{ background: profile.color }}
        />
        <span className="truncate text-[13px] font-medium tracking-[-0.005em] text-ink">{profile.name}</span>
        <SidebarSurfaceIcons surfaces={profile.surfaces} />
        {index < 9 ? (
          // See sibling `sidebar-profile-row` for the wrapper-flex rationale.
          <span className="inline-flex items-center transition-opacity opacity-40 group-hover/row:opacity-100 group-data-[active=true]/row:opacity-100">
            <Kbd>⌘{index + 1}</Kbd>
          </span>
        ) : null}
      </button>
      {/* Grip — absolute on the row's left edge inside the padded gutter.
          Owns the drag listeners so dragging it doesn't fire the button
          click. Visible at low opacity by default so users discover it,
          full opacity on hover/focus. */}
      <button
        type="button"
        {...attributes}
        {...listeners}
        // aria-label must come AFTER the dnd-kit attribute spread so our
        // per-row label ("Reorder Personal") wins over the generic
        // role/aria-roledescription dnd-kit attaches to every sortable.
        aria-label={`Reorder ${profile.name}`}
        className={cn(
          'absolute top-1/2 left-1 grid h-6 w-4 -translate-y-1/2 cursor-grab place-items-center text-muted-strong outline-none',
          'opacity-45 transition-opacity duration-(--duration-snap) ease-(--ease-natural)',
          'group-hover/sortable:opacity-100 focus-visible:opacity-100',
          'active:cursor-grabbing',
        )}
      >
        <GripVertical className="h-3.5 w-3.5" strokeWidth={1.75} />
      </button>
    </div>
  )
}
