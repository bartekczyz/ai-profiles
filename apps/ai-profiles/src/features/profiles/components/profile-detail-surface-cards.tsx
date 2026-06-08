import type { ReactNode } from 'react'
import type { AppId } from '@/lib/app-registry'
import type { ProfilePaths, Surfaces } from '@/lib/types'

import { useShortcut } from '@/design'
import { useDependencies } from '@/features/dependencies/api/use-dependencies'
import { appSpecs } from '@/lib/app-registry'
import { openInFinder } from '@/lib/commands'

import { DeepLinkInfo } from './deep-link-info'
import { ProfileDetailSurfaceCard } from './profile-detail-surface-card'

type Props = {
  app: AppId
  paths: ProfilePaths
  surfaces: Surfaces
  cliCommandLabel: ReactNode
  onLaunchGui: () => Promise<unknown>
  onCopyCli: () => Promise<unknown>
  onError: (message: string | null) => void
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
  if (slashIndex === -1) {
    return filePath
  }
  return filePath.slice(slashIndex + 1)
}

/**
 * Renders the two surface cards (GUI / CLI). Owns the reveal-in-Finder
 * shortcut wiring and the CLI status-line composition. Receives launch
 * and copy callbacks from the parent so the same component works for
 * both managed profiles (which log activity via useProfileLastUsed) and
 * the default entry (which uses direct shell / clipboard calls).
 *
 * Path-loading is the parent's job — pass a fully-resolved ProfilePaths.
 */
export function ProfileDetailSurfaceCards({
  app,
  paths,
  surfaces,
  cliCommandLabel,
  onLaunchGui,
  onCopyCli,
  onError,
}: Props) {
  const dependencies = useDependencies()
  const spec = appSpecs[app]

  async function safeRun(action: () => Promise<unknown>) {
    try {
      await action()
      onError(null)
    } catch (caught) {
      onError(caught instanceof Error ? caught.message : String(caught))
    }
  }

  useShortcut('reveal-gui-data', () => safeRun(() => openInFinder(paths.guiDataDir)), {
    enabled: surfaces.gui,
  })
  useShortcut('reveal-gui-launcher', () => safeRun(() => openInFinder(paths.guiLauncherPath ?? '')), {
    enabled: surfaces.gui && paths.guiLauncherPath !== null,
  })
  useShortcut('reveal-cli-config', () => safeRun(() => openInFinder(paths.cliConfigDir)), {
    enabled: surfaces.cli,
  })
  useShortcut('reveal-cli-wrapper', () => safeRun(() => openInFinder(paths.cliWrapperPath ?? '')), {
    enabled: surfaces.cli && paths.cliWrapperPath !== null,
  })

  const cliStatusDetail =
    paths.cliWrapperPath === null
      ? `Stock ${spec.cliBinary} — no wrapper`
      : dependencies.deps.localBinOnPath
        ? `Wrapper installed · ${basename(paths.cliWrapperPath)} on PATH`
        : `Wrapper installed · PATH needs ~/.local/bin (Settings → Shell PATH)`
  const cliStatusTone: 'success' | 'warning' =
    paths.cliWrapperPath === null ? 'success' : dependencies.deps.localBinOnPath ? 'success' : 'warning'

  const guiLauncherPath = paths.guiLauncherPath
  const cliWrapperPath = paths.cliWrapperPath

  const guiStatusDetail = guiLauncherPath === null ? undefined : `Launcher ready in ${shorten(guiLauncherPath)}`

  return (
    <>
      <ProfileDetailSurfaceCard
        variant="gui"
        app={app}
        enabled={surfaces.gui}
        primaryLabel={`Open ${spec.displayName}`}
        primaryKbd="⏎"
        statusDetail={guiStatusDetail}
        primarySuffix={surfaces.gui ? <DeepLinkInfo /> : null}
        secondaries={[
          { label: 'Reveal app', kbd: '⌥1', onClick: () => safeRun(() => openInFinder(paths.guiDataDir)) },
          ...(guiLauncherPath !== null
            ? [
                {
                  label: 'Launcher',
                  kbd: '⌥2',
                  onClick: () => safeRun(() => openInFinder(guiLauncherPath)),
                },
              ]
            : []),
        ]}
        onPrimary={() => safeRun(onLaunchGui)}
      />
      <ProfileDetailSurfaceCard
        variant="cli"
        app={app}
        enabled={surfaces.cli}
        primaryLabel={<>Copy {cliCommandLabel}</>}
        primaryKbd="⌘C"
        statusDetail={cliStatusDetail}
        statusTone={cliStatusTone}
        secondaries={[
          { label: 'Config', kbd: '⌥3', onClick: () => safeRun(() => openInFinder(paths.cliConfigDir)) },
          ...(cliWrapperPath !== null
            ? [
                {
                  label: 'Wrapper',
                  kbd: '⌥4',
                  onClick: () => safeRun(() => openInFinder(cliWrapperPath)),
                },
              ]
            : []),
        ]}
        onPrimary={() => safeRun(onCopyCli)}
      />
    </>
  )
}

type FallbackProps = {
  app: AppId
  surfaces: Surfaces
  cliCommandLabel: ReactNode
  onLaunchGui: () => Promise<unknown>
  onCopyCli: () => Promise<unknown>
}

/** Suspense fallback for the surface cards block — primary buttons render
 * without status detail. Mirrors the loaded shape so the layout doesn't
 * jump when the paths fetch resolves. */
export function ProfileDetailSurfaceCardsFallback({
  app,
  surfaces,
  cliCommandLabel,
  onLaunchGui,
  onCopyCli,
}: FallbackProps) {
  return (
    <>
      <ProfileDetailSurfaceCard
        variant="gui"
        app={app}
        enabled={surfaces.gui}
        primaryLabel={`Open ${appSpecs[app].displayName}`}
        primaryKbd="⏎"
        primarySuffix={surfaces.gui ? <DeepLinkInfo /> : null}
        secondaries={[
          { label: 'Reveal app', kbd: '⌥1' },
          { label: 'Launcher', kbd: '⌥2' },
        ]}
        onPrimary={onLaunchGui}
      />
      <ProfileDetailSurfaceCard
        variant="cli"
        app={app}
        enabled={surfaces.cli}
        primaryLabel={<>Copy {cliCommandLabel}</>}
        primaryKbd="⌘C"
        secondaries={[
          { label: 'Config', kbd: '⌥3' },
          { label: 'Wrapper', kbd: '⌥4' },
        ]}
        onPrimary={onCopyCli}
      />
    </>
  )
}
