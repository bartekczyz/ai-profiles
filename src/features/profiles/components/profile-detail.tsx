import type { Profile, Surface } from '@/lib/types'

import { Suspense, useState } from 'react'

import { useShortcut } from '@/design'
// cross-feature: detail pane reads dependency state to drive surface-card status lines
import { useDependencies } from '@/features/dependencies/api/use-dependencies'
import { openInFinder } from '@/lib/commands'

import { useProfilePaths } from '../api/use-profile-paths'
import { useProfileUsage } from '../api/use-profile-usage'
import { DeepLinkInfo } from './deep-link-info'
import { ProfileDetailDangerLink } from './profile-detail-danger-link'
import { ProfileDetailHeader } from './profile-detail-header'
import { ProfileDetailHintStrip } from './profile-detail-hint-strip'
import { ProfileDetailRecentActivity } from './profile-detail-recent-activity'
import { ProfileDetailSurfaceCard } from './profile-detail-surface-card'

type Props = {
  profile: Profile
  onEdit: () => void
  onDelete: () => void
  // Kept for future use (the More menu in Phase 11 wires it back in); the
  // surface card stays read-only for enable/disable in this phase.
  onToggle: (surface: Surface, enabled: boolean) => Promise<void>
}

function shorten(absolutePath: string): string {
  const home = absolutePath.match(/^\/Users\/[^/]+/)?.[0]
  if (home && absolutePath.startsWith(home)) {
    return `~${absolutePath.slice(home.length)}`
  }
  return absolutePath
}

function basename(filePath: string): string {
  const slashIndex = filePath.lastIndexOf('/')
  return slashIndex === -1 ? filePath : filePath.slice(slashIndex + 1)
}

/**
 * ProfileDetail itself does not call any suspending hook — every value
 * needed by the header, recent-activity block, danger link, and hint strip
 * already lives on the `profile` prop (which the sidebar gave us when the
 * user selected the row). The only thing that needs to be fetched is the
 * profile's on-disk paths (useProfilePaths) — that suspension is scoped
 * to <SurfaceCards/> below, so switching profiles never blanks the
 * surrounding chrome.
 */
export function ProfileDetail({ profile, onEdit, onDelete }: Props) {
  const [actionError, setActionError] = useState<string | null>(null)

  return (
    // Outer `<main>` is a flex column that DOESN'T scroll — only the inner
    // content area does. That keeps the hint strip pinned to the bottom of
    // the pane regardless of how tall the window is or how far the user
    // has scrolled.
    <main className="flex flex-1 flex-col overflow-hidden">
      <div className="flex-1 overflow-y-auto px-10 pt-10">
        <div className="mx-auto w-full max-w-[640px]">
          <ProfileDetailHeader profile={profile} onEdit={onEdit} />

          <div className="mb-6 grid grid-cols-1 gap-3.5 lg:grid-cols-2">
            {/* Re-keying on profile.id ensures the loaded paths state doesn't
                leak across profile switches — suspends fresh for each one. */}
            <Suspense key={profile.id} fallback={<SurfaceCardsFallback profile={profile} />}>
              <SurfaceCardsLoaded profile={profile} onError={setActionError} />
            </Suspense>
          </div>

          {actionError ? (
            <p role="alert" className="mb-4 text-meta text-red">
              {actionError}
            </p>
          ) : null}

          <ProfileDetailRecentActivity profileId={profile.id} />
          <ProfileDetailDangerLink onDelete={onDelete} />
        </div>
      </div>
      <ProfileDetailHintStrip />
    </main>
  )
}

type SurfaceCardsLoadedProps = {
  profile: Profile
  onError: (message: string | null) => void
}

function SurfaceCardsLoaded({ profile, onError }: SurfaceCardsLoadedProps) {
  const paths = useProfilePaths(profile.id)
  const dependencies = useDependencies()
  const usage = useProfileUsage()

  async function safeRun(action: () => Promise<unknown>) {
    try {
      await action()
      onError(null)
    } catch (caught) {
      onError(caught instanceof Error ? caught.message : String(caught))
    }
  }

  // Reveal-in-Finder shortcuts live here because they need `paths`. The
  // mount/unmount of effects via React 19's <Activity> means these
  // bindings drop automatically when the detail pane hides (e.g. user
  // switches to Settings) — no manual scope gate needed for that case.
  useShortcut('reveal-gui-data', () => safeRun(() => openInFinder(paths.guiDataDir)), {
    enabled: profile.surfaces.gui,
  })
  useShortcut('reveal-gui-launcher', () => safeRun(() => openInFinder(paths.guiLauncherPath)), {
    enabled: profile.surfaces.gui,
  })
  useShortcut('reveal-cli-config', () => safeRun(() => openInFinder(paths.cliConfigDir)), {
    enabled: profile.surfaces.cli,
  })
  useShortcut('reveal-cli-wrapper', () => safeRun(() => openInFinder(paths.cliWrapperPath)), {
    enabled: profile.surfaces.cli,
  })

  const cliStatusDetail = dependencies.deps.localBinOnPath
    ? `Wrapper installed · ${basename(paths.cliWrapperPath)} on PATH`
    : `Wrapper installed · PATH needs ~/.local/bin (Settings → Shell PATH)`
  const cliStatusTone: 'success' | 'warning' = dependencies.deps.localBinOnPath ? 'success' : 'warning'

  return (
    <>
      <ProfileDetailSurfaceCard
        variant="gui"
        enabled={profile.surfaces.gui}
        primaryLabel="Open Claude"
        primaryKbd="⏎"
        statusDetail={`Launcher ready in ${shorten(paths.guiLauncherPath)}`}
        primarySuffix={profile.surfaces.gui ? <DeepLinkInfo /> : null}
        secondaries={[
          { label: 'Reveal app', kbd: '⌥1', onClick: () => safeRun(() => openInFinder(paths.guiDataDir)) },
          { label: 'Launcher', kbd: '⌥2', onClick: () => safeRun(() => openInFinder(paths.guiLauncherPath)) },
        ]}
        onPrimary={() => safeRun(() => usage.launchDesktop(profile.id))}
      />
      <ProfileDetailSurfaceCard
        variant="cli"
        enabled={profile.surfaces.cli}
        primaryLabel={
          <>
            Copy <code className="font-mono">claude-{profile.slug}</code>
          </>
        }
        primaryKbd="⌘C"
        statusDetail={cliStatusDetail}
        statusTone={cliStatusTone}
        secondaries={[
          { label: 'Config', kbd: '⌥3', onClick: () => safeRun(() => openInFinder(paths.cliConfigDir)) },
          { label: 'Wrapper', kbd: '⌥4', onClick: () => safeRun(() => openInFinder(paths.cliWrapperPath)) },
        ]}
        onPrimary={() => safeRun(() => usage.copyCli({ profileId: profile.id, command: `claude-${profile.slug}` }))}
      />
    </>
  )
}

function SurfaceCardsFallback({ profile }: { profile: Profile }) {
  const usage = useProfileUsage()
  return (
    <>
      <ProfileDetailSurfaceCard
        variant="gui"
        enabled={profile.surfaces.gui}
        primaryLabel="Open Claude"
        primaryKbd="⏎"
        primarySuffix={profile.surfaces.gui ? <DeepLinkInfo /> : null}
        secondaries={[
          { label: 'Reveal app', kbd: '⌥1' },
          { label: 'Launcher', kbd: '⌥2' },
        ]}
        onPrimary={() => usage.launchDesktop(profile.id)}
      />
      <ProfileDetailSurfaceCard
        variant="cli"
        enabled={profile.surfaces.cli}
        primaryLabel={
          <>
            Copy <code className="font-mono">claude-{profile.slug}</code>
          </>
        }
        primaryKbd="⌘C"
        secondaries={[
          { label: 'Config', kbd: '⌥3' },
          { label: 'Wrapper', kbd: '⌥4' },
        ]}
        onPrimary={() => usage.copyCli({ profileId: profile.id, command: `claude-${profile.slug}` })}
      />
    </>
  )
}
