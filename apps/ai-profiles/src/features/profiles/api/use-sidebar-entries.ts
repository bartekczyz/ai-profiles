import type { AppId } from '@/lib/app-registry'
import type { DefaultEntry, ExistingInstallInfo, SidebarEntry } from '@/lib/types'

import { appIds, appSpecs } from '@/lib/app-registry'
import { useAppState } from '@/lib/app-state/use-app-state'

import { useMigration } from '../../migration/api/use-migration'
import { useProfiles } from './use-profiles'

/**
 * Composes the sidebar's entry list from two sources: the managed-profile
 * list (CRUD-backed by the Rust store) and synthetic "default" entries
 * derived from per-app existing-install detection. Default entries — one
 * per detected stock install — always precede the managed list, Claude
 * before ChatGPT (per `appIds` order).
 */
export function useSidebarEntries(): Array<SidebarEntry> {
  const { profiles } = useProfiles()
  const claudeMigration = useMigration('claude')
  const codexMigration = useMigration('codex')
  const { state: appState } = useAppState()

  const existingByApp: Record<AppId, ExistingInstallInfo> = {
    claude: claudeMigration.existing,
    codex: codexMigration.existing,
  }

  const defaults = makeDefaultEntries(existingByApp, appState.defaultProfileNames, appState.defaultProfileColors)
  const managed: Array<SidebarEntry> = profiles.map((profile) => ({ kind: 'managed', profile }))
  const defaultEntries: Array<SidebarEntry> = defaults.map((entry) => ({ kind: 'default', entry }))
  return [...defaultEntries, ...managed]
}

/**
 * Resolves the id of an entry regardless of which arm of the union it is.
 * Exported for callers that need to compare an entry against a stored id
 * (app.tsx selection routing, useSidebarSelection's match check, etc).
 */
export function entryId(entry: SidebarEntry): string {
  return entry.kind === 'managed' ? entry.profile.id : entry.entry.id
}

/**
 * Resolves the app of an entry regardless of which arm of the union it is.
 * Used by the sidebar's mixed-app glyph gating and app.tsx's window-tint
 * effect so both derive the app the same way.
 */
export function appFromEntry(entry: SidebarEntry): AppId {
  return entry.kind === 'managed' ? entry.profile.app : entry.entry.app
}

export type SidebarGroup = {
  app: AppId
  default: Extract<SidebarEntry, { kind: 'default' }> | null
  managed: Array<Extract<SidebarEntry, { kind: 'managed' }>>
}

/**
 * Pure: the entries ⌘1…⌘9 select, in the order the sidebar shows them: each
 * app's Default first, then its profiles, app by app. The sidebar's ⌘N badges
 * and app.tsx's bindings both read this, so the two can't disagree.
 */
export function shortcutEntries(entries: Array<SidebarEntry>): Array<SidebarEntry> {
  return groupEntriesByApp(entries)
    .flatMap((group) => [...(group.default ? [group.default] : []), ...group.managed])
    .slice(0, 9)
}

/**
 * Pure: groups sidebar entries by app in `appIds` order (Claude before ChatGPT).
 * Each group carries its synthetic default entry (if any) and its managed
 * profiles in their incoming (store) order. Apps with no entries are omitted.
 * The sidebar renders one section per group.
 */
export function groupEntriesByApp(entries: Array<SidebarEntry>): Array<SidebarGroup> {
  const groups: Array<SidebarGroup> = []
  for (const appId of appIds) {
    const defaultEntry =
      entries.find(
        (entry): entry is Extract<SidebarEntry, { kind: 'default' }> =>
          entry.kind === 'default' && entry.entry.app === appId,
      ) ?? null
    const managed = entries.filter(
      (entry): entry is Extract<SidebarEntry, { kind: 'managed' }> =>
        entry.kind === 'managed' && entry.profile.app === appId,
    )
    if (defaultEntry === null && managed.length === 0) {
      continue
    }
    groups.push({ app: appId, default: defaultEntry, managed })
  }
  return groups
}

/**
 * Pure: builds one synthetic default entry per app that has a detected
 * stock install. Returns entries in `appIds` order (Claude before ChatGPT).
 * `customNames` and `customColors` carry the names and colours the user gave
 * those entries, if any.
 * Exposed for unit-testing in isolation.
 */
export function makeDefaultEntries(
  existingByApp: Record<AppId, ExistingInstallInfo>,
  customNames: Partial<Record<AppId, string>> = {},
  customColors: Partial<Record<AppId, string>> = {},
): Array<DefaultEntry> {
  const entries: Array<DefaultEntry> = []
  for (const appId of appIds) {
    const existing = existingByApp[appId]
    const gui = existing.guiPath !== null
    const cli = existing.cliPath !== null
    if (!gui && !cli) {
      continue
    }
    const customName = customNames[appId] ?? null
    entries.push({
      id: `default:${appId}`,
      app: appId,
      name: customName ?? appSpecs[appId].displayName,
      customName,
      color: customColors[appId] ?? null,
      surfaces: { gui, cli },
    })
  }
  return entries
}
