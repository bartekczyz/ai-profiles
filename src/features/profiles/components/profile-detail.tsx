import type { Profile, ProfilePaths, Surface } from '@/lib/types'

import { useEffect, useState } from 'react'

import { Button } from '@/design/ui/button'
import { copyToClipboard, profilePaths as fetchProfilePaths, openInFinder, openProfileInApp } from '@/lib/commands'

import { DeepLinkInfo } from './deep-link-info'
import { SurfaceCard } from './surface-card'

type Props = {
  profile: Profile
  onEdit: () => void
  onDelete: () => void
  onToggle: (surface: Surface, enabled: boolean) => Promise<void>
}

export function ProfileDetail({ profile, onEdit, onDelete, onToggle }: Props) {
  const [paths, setPaths] = useState<ProfilePaths | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)

  useEffect(() => {
    setPaths(null)
    setActionError(null)
    fetchProfilePaths(profile.id)
      .then(setPaths)
      .catch((caught) => {
        setActionError(caught instanceof Error ? caught.message : String(caught))
      })
  }, [profile.id])

  async function safeRun(action: () => Promise<void>) {
    try {
      await action()
      setActionError(null)
    } catch (caught) {
      setActionError(caught instanceof Error ? caught.message : String(caught))
    }
  }

  return (
    <main className="flex-1 overflow-y-auto bg-background p-6">
      <header className="flex items-center gap-4">
        <span className="inline-block h-10 w-10 shrink-0 rounded-xl" style={{ background: profile.color }} />
        <div className="flex-1">
          <h2 className="text-xl font-semibold">{profile.name}</h2>
          <p className="text-xs text-muted-foreground">{profile.slug}</p>
        </div>
        <Button variant="ghost" size="sm" onClick={onEdit}>
          Edit
        </Button>
      </header>

      <div className="mt-6 space-y-4">
        <SurfaceCard
          title="Desktop App"
          description="Launches /Applications/Claude.app with an isolated user-data directory."
          enabled={profile.surfaces.gui}
          onToggle={(next) => onToggle('gui', next)}
          primaryAction={
            profile.surfaces.gui
              ? {
                  label: 'Open Desktop App',
                  onClick: () => safeRun(() => openProfileInApp(profile.id)),
                }
              : undefined
          }
          primarySuffix={profile.surfaces.gui ? <DeepLinkInfo /> : null}
          secondaryActions={
            profile.surfaces.gui && paths
              ? [
                  {
                    label: 'Reveal data',
                    onClick: () => safeRun(() => openInFinder(paths.guiDataDir)),
                  },
                  {
                    label: 'Reveal launcher',
                    onClick: () => safeRun(() => openInFinder(paths.guiLauncherPath)),
                  },
                ]
              : undefined
          }
        />

        <SurfaceCard
          title="Claude Code CLI"
          description="Wraps the `claude` binary with CLAUDE_CONFIG_DIR pointed at this profile."
          enabled={profile.surfaces.cli}
          onToggle={(next) => onToggle('cli', next)}
          primaryAction={
            profile.surfaces.cli
              ? {
                  label: `Copy: claude-${profile.slug}`,
                  onClick: () => safeRun(() => copyToClipboard(`claude-${profile.slug}`)),
                }
              : undefined
          }
          secondaryActions={
            profile.surfaces.cli && paths
              ? [
                  {
                    label: 'Reveal config',
                    onClick: () => safeRun(() => openInFinder(paths.cliConfigDir)),
                  },
                  {
                    label: 'Reveal wrapper',
                    onClick: () => safeRun(() => openInFinder(paths.cliWrapperPath)),
                  },
                ]
              : undefined
          }
        />

        {actionError ? <p className="text-sm text-red">{actionError}</p> : null}

        <section className="rounded-[10px] border border-red/40 p-4">
          <h3 className="text-sm font-semibold text-red">Danger zone</h3>
          <p className="mt-1 text-xs text-muted-foreground">
            Deleting a profile removes its launchers and data directory.
          </p>
          <Button variant="ghost" size="sm" className="mt-3 text-red" onClick={onDelete}>
            Delete profile
          </Button>
        </section>
      </div>
    </main>
  )
}
