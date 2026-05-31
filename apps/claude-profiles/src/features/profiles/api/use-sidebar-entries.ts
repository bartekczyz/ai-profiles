import type { AppId } from '@/lib/app-registry'
import type { DefaultEntry, ExistingInstallInfo, SidebarEntry } from '@/lib/types'

import { appIds, appSpecs } from '@/lib/app-registry'

import { useMigration } from '../../migration/api/use-migration'
import { useProfiles } from './use-profiles'

/**
 * Composes the sidebar's entry list from two sources: the managed-profile
 * list (CRUD-backed by the Rust store) and synthetic "default" entries
 * derived from per-app existing-install detection. Default entries — one
 * per detected stock install — always precede the managed list, Claude
 * before Codex (per `appIds` order).
 */
export function useSidebarEntries(): Array<SidebarEntry> {
  const { profiles } = useProfiles()
  const claudeMigration = useMigration('claude')
  const codexMigration = useMigration('codex')

  const existingByApp: Record<AppId, ExistingInstallInfo> = {
    claude: claudeMigration.existing,
    codex: codexMigration.existing,
  }

  const defaults = makeDefaultEntries(existingByApp)
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
 * Pure: builds one synthetic default entry per app that has a detected
 * stock install. Returns entries in `appIds` order (Claude before Codex).
 * Exposed for unit-testing in isolation.
 */
export function makeDefaultEntries(existingByApp: Record<AppId, ExistingInstallInfo>): Array<DefaultEntry> {
  const entries: Array<DefaultEntry> = []
  for (const appId of appIds) {
    const existing = existingByApp[appId]
    const gui = existing.guiPath !== null
    const cli = existing.cliPath !== null
    if (!gui && !cli) {
      continue
    }
    entries.push({
      id: `default:${appId}`,
      app: appId,
      name: appSpecs[appId].displayName,
      surfaces: { gui, cli },
    })
  }
  return entries
}
