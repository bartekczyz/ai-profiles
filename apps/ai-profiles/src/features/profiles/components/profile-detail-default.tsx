import type { DefaultEntry, ProfilePaths } from '@/lib/types'

import { Suspense, useState } from 'react'

import { appSpecs } from '@/lib/app-registry'
import { useAppState } from '@/lib/app-state/use-app-state'
import { copyToClipboard, openDefaultGui, profilePaths } from '@/lib/commands'

import { useProfilePaths } from '../api/use-profile-paths'
import { EditDefaultProfileDialog } from './edit-default-profile-dialog'
import { BrandSwatch, ProfileDetailHeader, ProfileSwatch } from './profile-detail-header'
import { ProfileDetailInfo } from './profile-detail-info'
import { ProfileDetailMigrateAction } from './profile-detail-migrate-action'
import { ProfileDetailOverflowMenu, ProfileDetailOverflowMenuFallback } from './profile-detail-overflow-menu'
import { ProfileDetailShell } from './profile-detail-shell'
import { ProfileDetailSurfacesPanel } from './profile-detail-surfaces-panel'
import { ProfileDetailUsageCard } from './profile-detail-usage-card'

type Props = {
  entry: DefaultEntry
  onMigrate: () => void
}

/**
 * The stock-install pane. Same header, usage panel, and surfaces panel as a
 * managed profile — what differs is only what this entry can do, expressed
 * by which slots get filled:
 *
 * - no Edit and no Delete, because nothing about a stock install is ours to
 *   change or remove. They are absent, not disabled. Only its label can be
 *   changed, via Rename in the overflow menu;
 * - Import leads the header action group in Edit's seat, since bringing the
 *   install under management is this entry's one unique action;
 * - the terminal row carries the plain CLI binary, not a `claude-<slug>`
 *   wrapper, because no wrapper exists.
 *
 * Usage renders unchanged: a stock install signs in like any other and has
 * the same quota to show.
 */
export function DefaultProfileDetail({ entry, onMigrate }: Props) {
  const [actionError, setActionError] = useState<string | null>(null)
  const [editing, setEditing] = useState(false)
  const appState = useAppState()
  const displayName = appSpecs[entry.app].displayName
  return (
    <ProfileDetailShell>
      <ProfileDetailHeader
        name={entry.customName ?? displayName}
        swatch={entry.color ? <ProfileSwatch color={entry.color} /> : <BrandSwatch app={entry.app} />}
        action={<ProfileDetailMigrateAction onMigrate={onMigrate} />}
        subline={entry.customName === null ? 'stock install' : `${displayName} · stock install`}
        info={<ProfileDetailInfo app={entry.app} />}
        menu={
          // Its own boundary, as on the managed pane: the identity block and
          // Import render immediately and only the paths behind the reveal
          // destinations wait. No `onDelete` — a stock install is not ours to
          // remove, so the menu is reveal-only.
          <Suspense key={entry.id} fallback={<ProfileDetailOverflowMenuFallback />}>
            <ProfileDetailOverflowMenu profileId={entry.id} onError={setActionError} onEdit={() => setEditing(true)} />
          </Suspense>
        }
      />

      <ProfileDetailUsageCard app={entry.app} profileId={entry.id} cliEnabled={entry.surfaces.cli} />

      <div className="mb-6">
        <Suspense key={entry.id} fallback={<DefaultSurfaces entry={entry} onError={setActionError} />}>
          <ResolvedDefaultSurfaces entry={entry} onError={setActionError} />
        </Suspense>
      </div>

      {actionError ? (
        <p role="alert" className="mb-4 text-meta text-red">
          {actionError}
        </p>
      ) : null}

      <EditDefaultProfileDialog
        open={editing}
        entry={entry}
        onClose={() => setEditing(false)}
        onSave={async ({ name, color }) => {
          await appState.update({
            ...(name === undefined ? {} : { defaultProfileName: { app: entry.app, name } }),
            ...(color === undefined ? {} : { defaultProfileColor: { app: entry.app, color } }),
          })
        }}
      />
    </ProfileDetailShell>
  )
}

type DefaultSurfacesProps = {
  entry: DefaultEntry
  /**
   * Absent while the stock install's paths are still resolving.
   */
  paths?: ProfilePaths
  onError: (message: string | null) => void
}

/**
 * The stock install's surfaces. Two capability differences from a managed
 * profile: the command is the bare CLI binary rather than a per-profile
 * wrapper, and the desktop row launches the vendor app against its own
 * stock data directory rather than a generated launcher.
 *
 * `shortcutsEnabled` is false because the app shell has never bound ⏎ / ⌘C
 * here — both are gated on a managed profile being selected.
 */
function DefaultSurfaces({ entry, paths, onError }: DefaultSurfacesProps) {
  const spec = appSpecs[entry.app]
  return (
    <ProfileDetailSurfacesPanel
      command={spec.cliBinary}
      guiEnabled={entry.surfaces.gui}
      cliEnabled={entry.surfaces.cli}
      shortcutsEnabled={false}
      guiDescription={paths === undefined ? undefined : guiDescription(paths, spec.guiBundleName)}
      cliDescription={paths === undefined ? undefined : `Stock ${spec.cliBinary} — no wrapper`}
      onLaunchGui={async () => {
        // Open is live from the first paint, but launching needs the stock
        // data directory, which arrives with the paths. Resolving it on
        // demand — rather than withholding the control, which would make the
        // row claim the surface was off — means an early click launches
        // instead of quietly doing nothing. The extra fetch only ever
        // happens inside that window; once the query has landed, `paths` is
        // already here.
        const resolved = paths ?? (await profilePaths(entry.id))
        if (resolved.guiLauncherPath === null) {
          throw new Error(`${spec.guiBundleName} isn't installed`)
        }
        await openDefaultGui(entry.app, resolved.guiDataDir)
      }}
      onCopyCli={async () => {
        await copyToClipboard(spec.cliBinary)
      }}
      onError={onError}
    />
  )
}

/** Suspends on the stock install's paths, which the launch and both descriptions need. */
function ResolvedDefaultSurfaces({ entry, onError }: Omit<DefaultSurfacesProps, 'paths'>) {
  const paths = useProfilePaths(entry.id)
  return <DefaultSurfaces entry={entry} paths={paths} onError={onError} />
}

function guiDescription(paths: ProfilePaths, guiBundleName: string): string {
  if (paths.guiLauncherPath === null) {
    return `${guiBundleName} not installed`
  }
  return `Stock ${guiBundleName}`
}
