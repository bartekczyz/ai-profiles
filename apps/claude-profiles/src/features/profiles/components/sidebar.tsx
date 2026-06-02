import type { Ref } from 'react'
import type { SidebarEntry } from '@/lib/types'
import type { SidebarGroup } from '../api/use-sidebar-entries'

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
import { appSpecs } from '@/lib/app-registry'

import { entryId, groupEntriesByApp } from '../api/use-sidebar-entries'
import { AppGlyph } from './app-glyph'
import { ManagedSidebarSwatch } from './managed-sidebar-swatch'
import { OutlinedSwatch } from './outlined-swatch'
import { SidebarBrandMark } from './sidebar-brand-mark'
import { SidebarProfileRow } from './sidebar-profile-row'
import { SidebarSearchInput } from './sidebar-search-input'
import { SortableProfileRow } from './sortable-profile-row'

type ManagedEntry = Extract<SidebarEntry, { kind: 'managed' }>

// Zeroing the X component locks drag motion to the vertical axis. The list
// is a column, so horizontal movement has no semantic meaning and only adds
// jitter — pin the row to its column the whole time.
const restrictToVerticalAxis: Modifier = ({ transform }) => ({ ...transform, x: 0 })

// Clamp the drag transform so the row can't be dragged past the top or
// bottom edge of the scrollable list container. The list has
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
  entries: Array<SidebarEntry>
  selectedId: string | null
  searchInputRef?: Ref<HTMLInputElement>
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

export function Sidebar({ entries, selectedId, searchInputRef, onSelect, onCreate, onSettings, onReorder }: Props) {
  const [query, setQuery] = useState('')

  const groups = groupEntriesByApp(entries)
  // App section headers appear only once the sidebar spans more than one app.
  const showHeaders = groups.length > 1

  // Flat managed list in store order — the source of truth for the ⌘N chip
  // index and for rebuilding the full order after a per-section reorder.
  const managedFlat: Array<ManagedEntry> = entries.filter((entry): entry is ManagedEntry => entry.kind === 'managed')

  // Reorder requires a handler and an unfiltered list — dragging within a
  // filtered list would produce a confusing result on the canonical order.
  const canReorder = onReorder !== undefined && query.trim().length === 0

  return (
    <aside className="relative flex w-64 shrink-0 flex-col border-r border-border bg-cream-2 px-3 pt-11 pb-3">
      <SidebarBrandMark />
      <SidebarSearchInput value={query} inputRef={searchInputRef} onChange={setQuery} />
      {showHeaders ? null : (
        <div className="px-2.5 pt-1.5 pb-2 font-mono text-[9.5px] font-medium uppercase tracking-[0.1em] text-muted-strong">
          Profiles
        </div>
      )}
      <div className="flex flex-1 flex-col gap-1.5 overflow-y-auto pr-0.5">
        {groups.map((group) => (
          <AppSection
            key={group.app}
            group={group}
            showHeader={showHeaders}
            selectedId={selectedId}
            query={query}
            canReorder={canReorder}
            managedFlat={managedFlat}
            onSelect={onSelect}
            onReorder={onReorder}
          />
        ))}
      </div>
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

type AppSectionProps = {
  group: SidebarGroup
  showHeader: boolean
  selectedId: string | null
  query: string
  canReorder: boolean
  managedFlat: Array<ManagedEntry>
  onSelect: (id: string) => void
  onReorder?: (ids: Array<string>) => void
}

/**
 * One per-app section: an optional header, the app's default row (brand-icon
 * swatch, pinned/non-draggable), then its managed rows (colour swatch,
 * drag-to-reorder within the section). Managed reorder is confined to the
 * section; the resulting full order threads the reordered ids back through the
 * flat store order so non-section profiles keep their positions.
 */
function AppSection({
  group,
  showHeader,
  selectedId,
  query,
  canReorder,
  managedFlat,
  onSelect,
  onReorder,
}: AppSectionProps) {
  const sensors = useSensors(
    // 6px activation distance means a normal click still selects; only
    // sustained drag motion starts a reorder.
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )

  const trimmedQuery = query.trim().toLowerCase()
  const matches = (name: string) => trimmedQuery.length === 0 || name.toLowerCase().includes(trimmedQuery)

  // Under a section header the default row just reads "Default" — the header
  // already names the app it belongs to. (entry.name stays the app name for
  // surfaces without grouping, e.g. the command palette.)
  const defaultRowName = 'Default'
  const visibleDefault = group.default !== null && matches(defaultRowName) ? group.default : null
  const visibleManaged = group.managed.filter((managedEntry) => matches(managedEntry.profile.name))

  if (visibleDefault === null && visibleManaged.length === 0) {
    return null
  }

  const shortcutIndexFor = (id: string) => managedFlat.findIndex((managedEntry) => managedEntry.profile.id === id)
  const reorderable = canReorder && group.managed.length > 1

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event
    if (!over || active.id === over.id || !onReorder) {
      return
    }
    const ids = group.managed.map((managedEntry) => managedEntry.profile.id)
    const oldIndex = ids.indexOf(String(active.id))
    const newIndex = ids.indexOf(String(over.id))
    if (oldIndex === -1 || newIndex === -1) {
      return
    }
    const reordered = [...ids]
    const [moved] = reordered.splice(oldIndex, 1)
    reordered.splice(newIndex, 0, moved)
    // Thread the reordered ids back through the flat store order, keeping
    // every other app's profiles in their existing positions.
    let cursor = 0
    const fullOrder = managedFlat.map((managedEntry) =>
      managedEntry.profile.app === group.app ? reordered[cursor++] : managedEntry.profile.id,
    )
    onReorder(fullOrder)
  }

  return (
    <section className="flex flex-col gap-px">
      {showHeader ? (
        <div className="flex items-center gap-1.5 px-2.5 pt-1 pb-1">
          <AppGlyph app={group.app} size={13} />
          <span className="font-mono text-[9.5px] font-medium uppercase tracking-[0.1em] text-muted-strong">
            {appSpecs[group.app].displayName}
          </span>
        </div>
      ) : null}

      {visibleDefault ? (
        <SidebarProfileRow
          name={defaultRowName}
          swatch={<OutlinedSwatch size={10} />}
          surfaces={visibleDefault.entry.surfaces}
          selected={entryId(visibleDefault) === selectedId}
          onSelect={() => onSelect(entryId(visibleDefault))}
        />
      ) : null}

      {reorderable ? (
        <DndContext
          sensors={sensors}
          collisionDetection={closestCenter}
          modifiers={[restrictToVerticalAxis, restrictToScrollableAncestor]}
          onDragEnd={handleDragEnd}
          accessibility={{
            announcements: {
              onDragStart: ({ active }) => `Picked up ${activeName(group.managed, active.id)}`,
              onDragOver: ({ active, over }) =>
                over
                  ? `${activeName(group.managed, active.id)} moved over ${activeName(group.managed, over.id)}`
                  : `${activeName(group.managed, active.id)} is no longer over a droppable area`,
              onDragEnd: ({ active, over }) =>
                over
                  ? `${activeName(group.managed, active.id)} dropped onto ${activeName(group.managed, over.id)}`
                  : `${activeName(group.managed, active.id)} drop cancelled`,
              onDragCancel: ({ active }) => `Drag of ${activeName(group.managed, active.id)} cancelled`,
            },
          }}
        >
          <SortableContext
            items={group.managed.map((managedEntry) => managedEntry.profile.id)}
            strategy={verticalListSortingStrategy}
          >
            <ul aria-label={`${appSpecs[group.app].displayName} profiles`} className="flex flex-col gap-px">
              {group.managed.map((managedEntry) => (
                <li key={managedEntry.profile.id}>
                  <SortableProfileRow
                    name={managedEntry.profile.name}
                    swatch={<ManagedSidebarSwatch color={managedEntry.profile.color} />}
                    surfaces={managedEntry.profile.surfaces}
                    selected={managedEntry.profile.id === selectedId}
                    shortcutIndex={shortcutIndexFor(managedEntry.profile.id)}
                    sortableId={managedEntry.profile.id}
                    onSelect={() => onSelect(managedEntry.profile.id)}
                  />
                </li>
              ))}
            </ul>
          </SortableContext>
        </DndContext>
      ) : (
        <ul aria-label={`${appSpecs[group.app].displayName} profiles`} className="flex flex-col gap-px">
          {visibleManaged.map((managedEntry) => (
            <li key={managedEntry.profile.id}>
              <SidebarProfileRow
                name={managedEntry.profile.name}
                swatch={<ManagedSidebarSwatch color={managedEntry.profile.color} />}
                surfaces={managedEntry.profile.surfaces}
                selected={managedEntry.profile.id === selectedId}
                shortcutIndex={shortcutIndexFor(managedEntry.profile.id)}
                onSelect={() => onSelect(managedEntry.profile.id)}
              />
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

function activeName(managedEntries: Array<ManagedEntry>, id: string | number): string {
  const match = managedEntries.find((managedEntry) => managedEntry.profile.id === id)
  return match ? match.profile.name : String(id)
}
