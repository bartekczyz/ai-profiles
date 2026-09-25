import type { SidebarEntry } from '@/lib/types'
import type { SidebarGroup } from '../api/use-sidebar-entries'

/**
 * A sidebar entry for a profile ai-profiles manages.
 */
export type ManagedEntry = Extract<SidebarEntry, { kind: 'managed' }>

/**
 * The rows of one app's sidebar section that match the search.
 */
type VisibleSection = {
  /**
   * What the default row reads: the user's name for it, else "Default".
   */
  defaultRowName: string
  /**
   * The app's default entry, or `null` when it has none or it doesn't match.
   */
  visibleDefault: SidebarGroup['default']
  /**
   * The app's managed entries that match, in store order.
   */
  visibleManaged: Array<ManagedEntry>
}

/**
 * Pure: the rows of `group` whose names contain `query`, ignoring case and
 * surrounding space. A blank query matches every row.
 */
export function visibleSection(group: SidebarGroup, query: string): VisibleSection {
  const trimmedQuery = query.trim().toLowerCase()
  const matches = (name: string) => trimmedQuery.length === 0 || name.toLowerCase().includes(trimmedQuery)

  // The row reads just "Default" — the app it belongs to is stated by the
  // glyph in its leading column — unless the user has renamed it. (Without a
  // custom name, entry.name stays the app name for surfaces without grouping,
  // e.g. the command palette.)
  const defaultRowName = group.default?.entry.customName ?? 'Default'
  const visibleDefault = group.default !== null && matches(defaultRowName) ? group.default : null
  const visibleManaged = group.managed.filter((managedEntry) => matches(managedEntry.profile.name))
  return { defaultRowName, visibleDefault, visibleManaged }
}

/**
 * Pure: the full managed order after dragging `activeId` onto `overId` within
 * `group`, or `null` when the drop changes nothing (onto itself, or either
 * end outside the section). The reordered section is threaded back through
 * the flat store order, so every other app's profiles keep their positions.
 */
export function reorderedProfileIds(
  managedFlat: Array<ManagedEntry>,
  group: SidebarGroup,
  activeId: string,
  overId: string,
): Array<string> | null {
  const ids = group.managed.map((managedEntry) => managedEntry.profile.id)
  const oldIndex = ids.indexOf(activeId)
  const newIndex = ids.indexOf(overId)
  if (activeId === overId || oldIndex === -1 || newIndex === -1) {
    return null
  }
  const reordered = [...ids]
  const [moved] = reordered.splice(oldIndex, 1)
  reordered.splice(newIndex, 0, moved)
  let cursor = 0
  return managedFlat.map((managedEntry) =>
    managedEntry.profile.app === group.app ? reordered[cursor++] : managedEntry.profile.id,
  )
}
