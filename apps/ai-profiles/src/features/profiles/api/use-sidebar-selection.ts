import type { SidebarEntry } from '@/lib/types'

import { useEffect, useRef, useState } from 'react'

import { useAppState } from '@/lib/app-state/use-app-state'

import { entryId } from './use-sidebar-entries'

const persistDebounceMs = 300

type Result = {
  selectedId: string | null
  select: (id: string | null) => void
}

/**
 * Owns selection state across the sidebar entry list. Restores the
 * last-selected id from AppState on first render. Persists changes via
 * a debounced AppState update so a burst of selection changes (rapid
 * ⌘1..⌘9 keystrokes) collapses into a single IPC write.
 *
 * Falls back to entries[0] when the persisted id no longer matches any
 * entry (e.g. the user deleted the selected profile, or migration
 * removed the default row).
 *
 * `isExtraId` names ids that are valid without being entries — a remote
 * host's account — so selecting one isn't undone as stale.
 */
export function useSidebarSelection(
  entries: Array<SidebarEntry>,
  isExtraId: (id: string) => boolean = () => false,
): Result {
  const appState = useAppState()
  const persisted = appState.state.selectedEntryId

  const [selectedId, setSelectedId] = useState<string | null>(() => {
    if (persisted && (entries.some((entry) => entryId(entry) === persisted) || isExtraId(persisted))) {
      return persisted
    }
    return entries.length > 0 ? entryId(entries[0]) : null
  })

  // Re-fall-back when the selected entry vanishes (entry deleted,
  // default disappears post-migration, etc). The new fallback id is
  // intentionally not persisted — leaving the stale id in AppState is
  // self-healing on next mount (it just falls back again).
  useEffect(() => {
    if (selectedId === null) {
      if (entries.length > 0) {
        setSelectedId(entryId(entries[0]))
      }
      return
    }
    const present = entries.some((entry) => entryId(entry) === selectedId) || isExtraId(selectedId)
    if (!present) {
      setSelectedId(entries.length > 0 ? entryId(entries[0]) : null)
    }
  }, [entries, selectedId, isExtraId])

  // Debounced AppState persistence — only fires on user-initiated
  // changes, not the initial restore or effect-driven fallback.
  const writeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const lastWrittenRef = useRef<string | null>(persisted)

  function select(id: string | null) {
    setSelectedId(id)
    if (writeTimerRef.current !== null) {
      clearTimeout(writeTimerRef.current)
    }
    writeTimerRef.current = setTimeout(() => {
      if (id === lastWrittenRef.current) {
        return
      }
      lastWrittenRef.current = id
      if (id === null) {
        void appState.update({ clearSelectedEntryId: true })
        return
      }
      void appState.update({ selectedEntryId: id })
    }, persistDebounceMs)
  }

  useEffect(() => {
    return () => {
      if (writeTimerRef.current !== null) {
        clearTimeout(writeTimerRef.current)
      }
    }
  }, [])

  return { selectedId, select }
}
