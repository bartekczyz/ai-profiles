import type { AppId } from '@/lib/app-registry'
import type { Profile } from '@/lib/types'
import type { GuiLaunch } from './use-gui-launch'

import { Suspense, useState } from 'react'

import { PaneLayout } from '@/components/pane-layout'
import { useDependencies } from '@/features/dependencies/api/use-dependencies'
import { appSpecs, wrapperCommand } from '@/lib/app-registry'

import { useProfileLastUsed } from '../api/use-profile-last-used'
import { useProfilePaths } from '../api/use-profile-paths'
import { AccountPart } from './account-line'
import { formatLastUsed } from './format-last-used'
import { ProfileDetailHeader, ProfileSwatch } from './profile-detail-header'
import { ProfileDetailInfo } from './profile-detail-info'
import { ProfileDetailOverflowMenu, ProfileDetailOverflowMenuFallback } from './profile-detail-overflow-menu'
import { ProfileDetailSessions } from './profile-detail-sessions'
import { ProfileDetailSurfacesPanel } from './profile-detail-surfaces-panel'
import { ProfileDetailUsageCard } from './profile-detail-usage-card'
import { useGuiLaunch } from './use-gui-launch'

type Props = {
  profile: Profile
  /**
   * Whether this pane's ⏎ / ⌘C bindings are live — false while a dialog,
   * the command palette, or the settings pane covers the detail pane.
   */
  shortcutsEnabled: boolean
  onEdit: () => void
  onDelete: () => void
}

export function ProfileDetail({ profile, shortcutsEnabled, onEdit, onDelete }: Props) {
  const [actionError, setActionError] = useState<string | null>(null)
  const command = wrapperCommand(profile.app, profile.slug)
  // Above the boundary below, which swaps one surfaces panel for another as
  // soon as the paths land.
  const launch = useGuiLaunch()

  return (
    <PaneLayout
      header={
        <ProfileDetailHeader
          name={profile.name}
          swatch={<ProfileSwatch color={profile.color} />}
          info={<ProfileDetailInfo app={profile.app} command={command} />}
          subline={
            <>
              <span>{appSpecs[profile.app].displayName}</span>
              <AccountPart profileId={profile.id} />
              <span className="mx-2 text-border">·</span>
              <span className="text-muted-strong">{formatLastUsed(profile.lastUsedAt)}</span>
            </>
          }
          menu={
            // Its own boundary: the header's identity block renders from
            // sidebar-provided data immediately, and only the menu waits on
            // the per-profile path resolution.
            <Suspense key={profile.id} fallback={<ProfileDetailOverflowMenuFallback />}>
              <ProfileDetailOverflowMenu profileId={profile.id} onDelete={onDelete} onError={setActionError} />
            </Suspense>
          }
          onEdit={onEdit}
        />
      }
    >
      <ProfileDetailUsageCard
        app={profile.app}
        cliCommand={command}
        cliEnabled={profile.surfaces.cli}
        profileId={profile.id}
      />

      {/* Only the two row descriptions wait on per-profile data, so the
          fallback is the same panel with its description slots empty: the
          controls stay live and nothing moves when the paths land. */}
      <div className="mb-6">
        <Suspense
          key={profile.id}
          fallback={
            <ManagedSurfaces
              profile={profile}
              shortcutsEnabled={shortcutsEnabled}
              launch={launch}
              onError={setActionError}
            />
          }
        >
          <ResolvedManagedSurfaces
            profile={profile}
            shortcutsEnabled={shortcutsEnabled}
            launch={launch}
            onError={setActionError}
          />
        </Suspense>
      </div>

      {profile.app === 'claude' ? <ProfileDetailSessions key={profile.id} profileId={profile.id} /> : null}

      {actionError ? (
        <p role="alert" className="mb-4 text-meta text-red">
          {actionError}
        </p>
      ) : null}
    </PaneLayout>
  )
}

type ManagedSurfacesProps = {
  profile: Profile
  shortcutsEnabled: boolean
  /**
   * The pane's desktop launch, held above the `Suspense` boundary so it
   * survives this panel being swapped for the resolved one.
   */
  launch: GuiLaunch
  /**
   * Absent while the data behind the row descriptions is still resolving.
   */
  guiDescription?: string
  cliDescription?: string
  onError: (message: string | null) => void
}

function ManagedSurfaces({
  profile,
  shortcutsEnabled,
  launch,
  guiDescription,
  cliDescription,
  onError,
}: ManagedSurfacesProps) {
  const lastUsed = useProfileLastUsed()
  const command = wrapperCommand(profile.app, profile.slug)
  return (
    <ProfileDetailSurfacesPanel
      command={command}
      guiEnabled={profile.surfaces.gui}
      cliEnabled={profile.surfaces.cli}
      shortcutsEnabled={shortcutsEnabled}
      opening={launch.opening}
      guiDescription={guiDescription}
      cliDescription={cliDescription}
      onLaunchGui={() => launch.run(() => lastUsed.launchDesktop(profile.id))}
      onCopyCli={() => lastUsed.copyCli({ profileId: profile.id, command })}
      onError={onError}
    />
  )
}

/**
 * Suspends on the profile's resolved paths and the shell-PATH check — the
 * only things the two row descriptions need.
 */
function ResolvedManagedSurfaces({
  profile,
  shortcutsEnabled,
  launch,
  onError,
}: Omit<ManagedSurfacesProps, 'guiDescription' | 'cliDescription'>) {
  const paths = useProfilePaths(profile.id)
  const dependencies = useDependencies()
  return (
    <ManagedSurfaces
      profile={profile}
      shortcutsEnabled={shortcutsEnabled}
      launch={launch}
      guiDescription={desktopDescription(profile.distinctDockIcon, paths.guiLauncherPath)}
      cliDescription={cliDescription(profile.app, paths.cliWrapperPath, dependencies.deps.localBinOnPath)}
      onError={onError}
    />
  )
}

function desktopDescription(distinctDockIcon: boolean, guiLauncherPath: string | null): string {
  if (guiLauncherPath === null) {
    return 'Launcher missing — re-save from Edit'
  }
  return distinctDockIcon ? 'Own Dock icon and name' : 'Isolated launcher installed'
}

function cliDescription(app: AppId, cliWrapperPath: string | null, localBinOnPath: boolean): string {
  if (cliWrapperPath === null) {
    return `Stock ${appSpecs[app].cliBinary} — no wrapper`
  }
  if (localBinOnPath) {
    return 'Wrapper on your PATH'
  }
  return 'Wrapper installed — add ~/.local/bin to PATH in Settings'
}
