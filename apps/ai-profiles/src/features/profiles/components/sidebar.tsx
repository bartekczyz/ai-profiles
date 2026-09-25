import type { ReactNode, Ref } from 'react'
import type { SidebarEntry } from '@/lib/types'
import type { SidebarGroup } from '../api/use-sidebar-entries'
import type { ManagedEntry } from '../lib/sidebar-section'

import { useState } from 'react'

import {
  type Announcements,
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
import { reorderedProfileIds, visibleSection } from '../lib/sidebar-section'
import { AppGlyph } from './app-glyph'
import { ManagedSidebarSwatch } from './managed-sidebar-swatch'
import { OutlinedSwatch } from './outlined-swatch'
import { SidebarBrandMark } from './sidebar-brand-mark'
import { SidebarProfileRow } from './sidebar-profile-row'
import { SidebarSearchInput } from './sidebar-search-input'
import { SortableProfileRow } from './sortable-profile-row'

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
  // The per-row app glyph only earns its column once the sidebar actually
  // spans more than one app — otherwise it would repeat the same mark on every
  // row and spend width the profile names need.
  const showAppGlyphs = groups.length > 1

  // Flat managed list in store order — the source of truth for the ⌘N chip
  // index and for rebuilding the full order after a per-section reorder.
  const managedFlat: Array<ManagedEntry> = entries.filter((entry): entry is ManagedEntry => entry.kind === 'managed')

  // Reorder requires a handler and an unfiltered list — dragging within a
  // filtered list would produce a confusing result on the canonical order.
  const canReorder = onReorder !== undefined && query.trim().length === 0

  return (
    <aside className="relative flex w-[200px] shrink-0 flex-col border-r border-border bg-cream-2 px-2 pt-11 pb-3">
      <SidebarBrandMark />
      <SidebarSearchInput value={query} inputRef={searchInputRef} onChange={setQuery} />
      {/* App groups are separated by a hairline rather than a text header. The
          rule is a sibling selector, not an index, so a group whose rows are
          all filtered out takes its separator with it. */}
      <div className="flex flex-1 flex-col overflow-y-auto pr-0.5 [&>section+section]:mt-[7px] [&>section+section]:border-t [&>section+section]:border-border [&>section+section]:pt-[7px]">
        {groups.map((group) => (
          <AppSection
            key={group.app}
            group={group}
            showAppGlyph={showAppGlyphs}
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
  showAppGlyph: boolean
  selectedId: string | null
  query: string
  canReorder: boolean
  managedFlat: Array<ManagedEntry>
  onSelect: (id: string) => void
  onReorder?: (ids: Array<string>) => void
}

/**
 * One per-app section: the app's default row (brand-icon swatch,
 * pinned/non-draggable), then its managed rows (colour swatch,
 * drag-to-reorder within the section). Managed reorder is confined to the
 * section; the resulting full order threads the reordered ids back through the
 * flat store order so non-section profiles keep their positions.
 *
 * The section carries no visible header — which app a row belongs to is stated
 * by the glyph on the row itself, and the sections are told apart by the
 * hairline the parent draws between them.
 */
function AppSection({
  group,
  showAppGlyph,
  selectedId,
  query,
  canReorder,
  managedFlat,
  onSelect,
  onReorder,
}: AppSectionProps) {
  const { defaultRowName, visibleDefault, visibleManaged } = visibleSection(group, query)

  if (visibleDefault === null && visibleManaged.length === 0) {
    return null
  }

  const reorderable = canReorder && group.managed.length > 1
  // Built once and handed to every row in the section — the mark is per-app,
  // not per-row, and a single-app sidebar suppresses it entirely.
  const rowGlyph = showAppGlyph ? <AppGlyph app={group.app} size={13} /> : null

  return (
    <section className="flex flex-col gap-px">
      {visibleDefault ? (
        <SidebarProfileRow
          name={defaultRowName}
          swatch={<OutlinedSwatch size={10} />}
          surfaces={visibleDefault.entry.surfaces}
          selected={entryId(visibleDefault) === selectedId}
          glyph={rowGlyph}
          onSelect={() => onSelect(entryId(visibleDefault))}
        />
      ) : null}

      {reorderable ? (
        <ReorderableManagedList
          group={group}
          managedFlat={managedFlat}
          selectedId={selectedId}
          glyph={rowGlyph}
          onSelect={onSelect}
          onReorder={onReorder}
        />
      ) : (
        <ul aria-label={`${appSpecs[group.app].displayName} profiles`} className="flex flex-col gap-px">
          {visibleManaged.map((managedEntry) => (
            <li key={managedEntry.profile.id}>
              <SidebarProfileRow
                name={managedEntry.profile.name}
                swatch={<ManagedSidebarSwatch color={managedEntry.profile.color} />}
                surfaces={managedEntry.profile.surfaces}
                selected={managedEntry.profile.id === selectedId}
                glyph={rowGlyph}
                shortcutIndex={shortcutIndexOf(managedFlat, managedEntry.profile.id)}
                onSelect={() => onSelect(managedEntry.profile.id)}
              />
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

type ReorderableManagedListProps = {
  /**
   * The app section whose managed rows are listed, all of them.
   */
  group: SidebarGroup
  /**
   * Every managed entry in store order, for the ⌘N chip index and the full
   * order after a reorder.
   */
  managedFlat: Array<ManagedEntry>
  /**
   * The selected entry's id, or `null` when none is selected.
   */
  selectedId: string | null
  /**
   * The app mark each row leads with, or `null` in a single-app sidebar.
   */
  glyph: ReactNode
  /**
   * Called with a row's id when the user selects it.
   */
  onSelect: (id: string) => void
  /**
   * Called with the full managed order after the user drags a row.
   */
  onReorder?: (ids: Array<string>) => void
}

/**
 * The section's managed rows as a drag-to-reorder list. Drag motion stays on
 * the vertical axis inside the scrolling list, and screen readers hear each
 * pick-up, move and drop by profile name.
 */
function ReorderableManagedList({
  group,
  managedFlat,
  selectedId,
  glyph,
  onSelect,
  onReorder,
}: ReorderableManagedListProps) {
  const sensors = useSensors(
    // 6px activation distance means a normal click still selects; only
    // sustained drag motion starts a reorder.
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event
    if (!over || !onReorder) {
      return
    }
    const fullOrder = reorderedProfileIds(managedFlat, group, String(active.id), String(over.id))
    if (fullOrder !== null) {
      onReorder(fullOrder)
    }
  }

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCenter}
      modifiers={[restrictToVerticalAxis, restrictToScrollableAncestor]}
      onDragEnd={handleDragEnd}
      accessibility={{ announcements: dragAnnouncements(group.managed) }}
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
                glyph={glyph}
                shortcutIndex={shortcutIndexOf(managedFlat, managedEntry.profile.id)}
                sortableId={managedEntry.profile.id}
                onSelect={() => onSelect(managedEntry.profile.id)}
              />
            </li>
          ))}
        </ul>
      </SortableContext>
    </DndContext>
  )
}

/**
 * The position of profile `id` in the flat managed store order — the index
 * its ⌘N chip shows.
 */
function shortcutIndexOf(managedFlat: Array<ManagedEntry>, id: string): number {
  return managedFlat.findIndex((managedEntry) => managedEntry.profile.id === id)
}

/**
 * What screen readers hear as a managed row is dragged, naming the profiles
 * rather than their ids.
 */
function dragAnnouncements(managedEntries: Array<ManagedEntry>): Announcements {
  return {
    onDragStart: ({ active }) => `Picked up ${activeName(managedEntries, active.id)}`,
    onDragOver: ({ active, over }) =>
      over
        ? `${activeName(managedEntries, active.id)} moved over ${activeName(managedEntries, over.id)}`
        : `${activeName(managedEntries, active.id)} is no longer over a droppable area`,
    onDragEnd: ({ active, over }) =>
      over
        ? `${activeName(managedEntries, active.id)} dropped onto ${activeName(managedEntries, over.id)}`
        : `${activeName(managedEntries, active.id)} drop cancelled`,
    onDragCancel: ({ active }) => `Drag of ${activeName(managedEntries, active.id)} cancelled`,
  }
}

function activeName(managedEntries: Array<ManagedEntry>, id: string | number): string {
  const match = managedEntries.find((managedEntry) => managedEntry.profile.id === id)
  return match ? match.profile.name : String(id)
}
