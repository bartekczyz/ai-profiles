import type { Profile } from '@/lib/types'

import { useState } from 'react'

import {
  closestCenter,
  DndContext,
  type DragEndEvent,
  KeyboardSensor,
  type Modifier,
  PointerSensor,
  useSensor,
  useSensors,
} from '@dnd-kit/core'
import { SortableContext, sortableKeyboardCoordinates, verticalListSortingStrategy } from '@dnd-kit/sortable'
import { Cog, Plus } from 'lucide-react'

import { ariaKeyshortcutsFor, Button, Kbd } from '@/design'

import { SidebarBrandMark } from './sidebar-brand-mark'
import { SidebarProfileRow } from './sidebar-profile-row'
import { SidebarSearchInput } from './sidebar-search-input'
import { SortableProfileRow } from './sortable-profile-row'

// Zeroing the X component locks drag motion to the vertical axis. The list
// is a column, so horizontal movement has no semantic meaning and only adds
// jitter — pin the row to its column the whole time.
const restrictToVerticalAxis: Modifier = ({ transform }) => ({ ...transform, x: 0 })

// Clamp the drag transform so the row can't be dragged past the top or
// bottom edge of the scrollable list container. The `<ul>` has
// `overflow-y-auto`, so it shows up as the first scrollable ancestor.
const restrictToScrollableAncestor: Modifier = ({ transform, draggingNodeRect, scrollableAncestorRects }) => {
  const container = scrollableAncestorRects[0]
  if (!draggingNodeRect || !container) {
    return transform
  }
  const minY = container.top - draggingNodeRect.top
  const maxY = container.top + container.height - draggingNodeRect.bottom
  return { ...transform, y: Math.min(Math.max(transform.y, minY), maxY) }
}

type Props = {
  profiles: Array<Profile>
  selectedId: string | null
  onSelect: (id: string) => void
  onCreate: () => void
  onSettings: () => void
  /**
   * Called with the new id sequence when the user drags to reorder. The
   * caller persists the order (via useProfiles().reorder). Optional:
   * when omitted, the rows render but drag-to-reorder is disabled.
   */
  onReorder?: (ids: Array<string>) => void
}

export function Sidebar({ profiles, selectedId, onSelect, onCreate, onSettings, onReorder }: Props) {
  const [query, setQuery] = useState('')
  const filtered =
    query.trim().length === 0
      ? profiles
      : profiles.filter((profile) => profile.name.toLowerCase().includes(query.trim().toLowerCase()))

  const sensors = useSensors(
    // 6px activation distance means a normal click still selects; only
    // sustained drag motion starts a reorder.
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event
    if (!over || active.id === over.id || !onReorder) {
      return
    }
    const oldIndex = profiles.findIndex((profile) => profile.id === active.id)
    const newIndex = profiles.findIndex((profile) => profile.id === over.id)
    if (oldIndex === -1 || newIndex === -1) {
      return
    }
    const next = [...profiles]
    const [moved] = next.splice(oldIndex, 1)
    next.splice(newIndex, 0, moved)
    onReorder(next.map((profile) => profile.id))
  }

  // Reorder requires (a) a handler, (b) an unfiltered list — dragging in a
  // filtered list would produce a confusing result on the canonical order,
  // and (c) at least two rows — there's nothing to reorder otherwise.
  const reorderable = onReorder !== undefined && query.trim().length === 0 && profiles.length > 1

  return (
    <aside className="relative flex w-64 shrink-0 flex-col border-r border-border bg-cream-2 px-3 pt-11 pb-3">
      <SidebarBrandMark />
      <SidebarSearchInput value={query} onChange={setQuery} />
      <div className="px-2.5 pt-1.5 pb-2 font-mono text-[9.5px] font-medium uppercase tracking-[0.1em] text-muted-strong">
        Profiles
      </div>
      {reorderable ? (
        <DndContext
          sensors={sensors}
          collisionDetection={closestCenter}
          modifiers={[restrictToVerticalAxis, restrictToScrollableAncestor]}
          onDragEnd={handleDragEnd}
          accessibility={{
            announcements: {
              onDragStart: ({ active }) => `Picked up ${activeName(profiles, active.id)}`,
              onDragOver: ({ active, over }) =>
                over
                  ? `${activeName(profiles, active.id)} moved over ${activeName(profiles, over.id)}`
                  : `${activeName(profiles, active.id)} is no longer over a droppable area`,
              onDragEnd: ({ active, over }) =>
                over
                  ? `${activeName(profiles, active.id)} dropped onto ${activeName(profiles, over.id)}`
                  : `${activeName(profiles, active.id)} drop cancelled`,
              onDragCancel: ({ active }) => `Drag of ${activeName(profiles, active.id)} cancelled`,
            },
          }}
        >
          <SortableContext items={profiles.map((profile) => profile.id)} strategy={verticalListSortingStrategy}>
            <ul aria-label="Profiles" className="flex flex-1 flex-col gap-px overflow-y-auto pr-0.5">
              {profiles.map((profile, index) => (
                <li key={profile.id}>
                  <SortableProfileRow
                    profile={profile}
                    index={index}
                    selected={profile.id === selectedId}
                    onSelect={() => onSelect(profile.id)}
                  />
                </li>
              ))}
            </ul>
          </SortableContext>
        </DndContext>
      ) : (
        <ul className="flex flex-1 flex-col gap-px overflow-y-auto pr-0.5">
          {filtered.map((profile) => {
            const index = profiles.indexOf(profile)
            return (
              <li key={profile.id}>
                <SidebarProfileRow
                  profile={profile}
                  index={index}
                  selected={profile.id === selectedId}
                  onSelect={() => onSelect(profile.id)}
                />
              </li>
            )
          })}
        </ul>
      )}
      <footer className="mt-2 flex items-center gap-2 border-t border-border pt-2.5">
        <Button
          variant="primary"
          size="sm"
          className="flex-1 rounded-full"
          leadingIcon={<Plus className="h-3.5 w-3.5" strokeWidth={2.25} />}
          trailingKbd={<Kbd variant="onOrange" shortcutId="open-create-profile" />}
          aria-keyshortcuts={ariaKeyshortcutsFor('open-create-profile')}
          onClick={onCreate}
        >
          New profile
        </Button>
        <button
          type="button"
          onClick={onSettings}
          aria-label="Open settings"
          aria-keyshortcuts={ariaKeyshortcutsFor('toggle-settings')}
          title="Settings (⌘,)"
          className="grid h-7 w-[30px] cursor-pointer place-items-center rounded-sm border border-border bg-white/60 text-muted transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:bg-white hover:text-ink dark:bg-white/[0.04] dark:hover:bg-white/[0.08] dark:hover:text-ink"
        >
          <Cog className="h-3.5 w-3.5" strokeWidth={1.75} />
        </button>
      </footer>
    </aside>
  )
}

function activeName(profiles: Array<Profile>, id: string | number): string {
  const match = profiles.find((profile) => profile.id === id)
  return match ? match.name : String(id)
}
