import type { ReactNode, Ref } from 'react'
import type { SidebarEntry } from '@/lib/types'

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

import { appFromEntry, entryId } from '../api/use-sidebar-entries'
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

// The swatch slot for a row: a per-app glyph when the sidebar spans more than
// one app-kind, otherwise the entry's own swatch (outlined for defaults, the
// colour dot for managed profiles). Centralised so the three render paths
// (defaults, sortable managed, static managed) can't drift.
function swatchFor(entry: SidebarEntry, showGlyphs: boolean): ReactNode {
  if (showGlyphs) {
    return <AppGlyph app={appFromEntry(entry)} />
  }
  if (entry.kind === 'default') {
    return <OutlinedSwatch size={10} />
  }
  return <ManagedSidebarSwatch color={entry.profile.color} />
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

  // Show per-app glyphs only when the sidebar contains entries from more than
  // one app-kind. One default Claude + one default Codex already triggers it.
  const presentApps = new Set(entries.map(appFromEntry))
  const showGlyphs = presentApps.size > 1

  const defaults = entries.filter(
    (entry): entry is Extract<SidebarEntry, { kind: 'default' }> => entry.kind === 'default',
  )
  const managed = entries.filter(
    (entry): entry is Extract<SidebarEntry, { kind: 'managed' }> => entry.kind === 'managed',
  )

  const trimmedQuery = query.trim().toLowerCase()
  const filteredDefaults =
    trimmedQuery.length === 0
      ? defaults
      : defaults.filter((entry) => entry.entry.name.toLowerCase().includes(trimmedQuery))
  const filteredManaged =
    trimmedQuery.length === 0
      ? managed
      : managed.filter((managedEntry) => managedEntry.profile.name.toLowerCase().includes(trimmedQuery))

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
    const oldIndex = managed.findIndex((managedEntry) => managedEntry.profile.id === active.id)
    const newIndex = managed.findIndex((managedEntry) => managedEntry.profile.id === over.id)
    if (oldIndex === -1 || newIndex === -1) {
      return
    }
    const next = [...managed]
    const [moved] = next.splice(oldIndex, 1)
    next.splice(newIndex, 0, moved)
    onReorder(next.map((managedEntry) => managedEntry.profile.id))
  }

  // Reorder requires (a) a handler, (b) an unfiltered list — dragging in a
  // filtered list would produce a confusing result on the canonical order,
  // and (c) at least two managed rows — there's nothing to reorder otherwise.
  const reorderable = onReorder !== undefined && query.trim().length === 0 && managed.length > 1

  return (
    <aside className="relative flex w-64 shrink-0 flex-col border-r border-border bg-cream-2 px-3 pt-11 pb-3">
      <SidebarBrandMark />
      <SidebarSearchInput value={query} inputRef={searchInputRef} onChange={setQuery} />
      <div className="px-2.5 pt-1.5 pb-2 font-mono text-[9.5px] font-medium uppercase tracking-[0.1em] text-muted-strong">
        Profiles
      </div>
      {/* Design choice: two separate <ul>s. Default entries are rendered in an
          unsorted list above the managed list. Only the managed <ul> is wrapped
          in a DndContext when reorderable — default rows are never draggable.
          Tab-order flows correctly: defaults first, then managed rows below. */}
      <div className="flex flex-1 flex-col gap-px overflow-y-auto pr-0.5">
        {filteredDefaults.length > 0 ? (
          <ul aria-label="Default profiles" className="flex flex-col gap-px">
            {filteredDefaults.map((defaultSidebarEntry) => (
              <li key={entryId(defaultSidebarEntry)}>
                <SidebarProfileRow
                  name={defaultSidebarEntry.entry.name}
                  swatch={swatchFor(defaultSidebarEntry, showGlyphs)}
                  surfaces={defaultSidebarEntry.entry.surfaces}
                  selected={entryId(defaultSidebarEntry) === selectedId}
                  onSelect={() => onSelect(entryId(defaultSidebarEntry))}
                />
              </li>
            ))}
          </ul>
        ) : null}
        {reorderable ? (
          <DndContext
            sensors={sensors}
            collisionDetection={closestCenter}
            modifiers={[restrictToVerticalAxis, restrictToScrollableAncestor]}
            onDragEnd={handleDragEnd}
            accessibility={{
              announcements: {
                onDragStart: ({ active }) => `Picked up ${activeName(managed, active.id)}`,
                onDragOver: ({ active, over }) =>
                  over
                    ? `${activeName(managed, active.id)} moved over ${activeName(managed, over.id)}`
                    : `${activeName(managed, active.id)} is no longer over a droppable area`,
                onDragEnd: ({ active, over }) =>
                  over
                    ? `${activeName(managed, active.id)} dropped onto ${activeName(managed, over.id)}`
                    : `${activeName(managed, active.id)} drop cancelled`,
                onDragCancel: ({ active }) => `Drag of ${activeName(managed, active.id)} cancelled`,
              },
            }}
          >
            <SortableContext
              items={managed.map((managedEntry) => managedEntry.profile.id)}
              strategy={verticalListSortingStrategy}
            >
              <ul aria-label="Managed profiles" className="flex flex-col gap-px">
                {managed.map((managedEntry, index) => (
                  <li key={managedEntry.profile.id}>
                    <SortableProfileRow
                      name={managedEntry.profile.name}
                      swatch={swatchFor(managedEntry, showGlyphs)}
                      surfaces={managedEntry.profile.surfaces}
                      selected={managedEntry.profile.id === selectedId}
                      shortcutIndex={index}
                      sortableId={managedEntry.profile.id}
                      onSelect={() => onSelect(managedEntry.profile.id)}
                    />
                  </li>
                ))}
              </ul>
            </SortableContext>
          </DndContext>
        ) : (
          <ul aria-label="Managed profiles" className="flex flex-col gap-px">
            {filteredManaged.map((managedEntry) => {
              const index = managed.indexOf(managedEntry)
              return (
                <li key={managedEntry.profile.id}>
                  <SidebarProfileRow
                    name={managedEntry.profile.name}
                    swatch={swatchFor(managedEntry, showGlyphs)}
                    surfaces={managedEntry.profile.surfaces}
                    selected={managedEntry.profile.id === selectedId}
                    shortcutIndex={index}
                    onSelect={() => onSelect(managedEntry.profile.id)}
                  />
                </li>
              )
            })}
          </ul>
        )}
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

function activeName(managedEntries: Array<Extract<SidebarEntry, { kind: 'managed' }>>, id: string | number): string {
  const match = managedEntries.find((managedEntry) => managedEntry.profile.id === id)
  return match ? match.profile.name : String(id)
}
