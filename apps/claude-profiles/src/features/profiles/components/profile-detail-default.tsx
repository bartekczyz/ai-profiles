import type { DefaultEntry } from '@/lib/types'

import { Suspense, useState } from 'react'

import { appSpecs } from '@/lib/app-registry'
import { copyToClipboard, openDefaultGui } from '@/lib/commands'

import { useProfilePaths } from '../api/use-profile-paths'
import { OutlinedSwatch } from './outlined-swatch'
import { ProfileDetailHeader } from './profile-detail-header'
import { ProfileDetailMigrateLink } from './profile-detail-migrate-link'
import { ProfileDetailShell } from './profile-detail-shell'
import { ProfileDetailSurfaceCards, ProfileDetailSurfaceCardsFallback } from './profile-detail-surface-cards'
import { ProfileDetailUsageCard } from './profile-detail-usage-card'

type Props = {
  entry: DefaultEntry
  onMigrate: () => void
}

export function DefaultProfileDetail({ entry, onMigrate }: Props) {
  const [actionError, setActionError] = useState<string | null>(null)
  const cliBinary = appSpecs[entry.app].cliBinary
  return (
    <ProfileDetailShell>
      <ProfileDetailHeader
        name="Default"
        swatch={<OutlinedSwatch size={44} />}
        subline={<code className="font-mono">{cliBinary}</code>}
      />

      <ProfileDetailUsageCard app={entry.app} profileId={entry.id} cliEnabled={entry.surfaces.cli} />

      <div className="mb-6 grid grid-cols-1 gap-3.5 lg:grid-cols-2">
        <Suspense key={entry.id} fallback={<DefaultSurfaceCardsFallback entry={entry} />}>
          <DefaultSurfaceCards entry={entry} onError={setActionError} />
        </Suspense>
      </div>

      {actionError ? (
        <p role="alert" className="mb-4 text-meta text-red">
          {actionError}
        </p>
      ) : null}

      <ProfileDetailMigrateLink onMigrate={onMigrate} />
    </ProfileDetailShell>
  )
}

type DefaultSurfaceCardsProps = {
  entry: DefaultEntry
  onError: (message: string | null) => void
}

function DefaultSurfaceCards({ entry, onError }: DefaultSurfaceCardsProps) {
  const paths = useProfilePaths(entry.id)
  const cliBinary = appSpecs[entry.app].cliBinary
  return (
    <ProfileDetailSurfaceCards
      app={entry.app}
      paths={paths}
      surfaces={entry.surfaces}
      cliCommandLabel={<code className="font-mono">{cliBinary}</code>}
      onLaunchGui={async () => {
        if (paths.guiLauncherPath === null) {
          return
        }
        await openDefaultGui(entry.app, paths.guiDataDir)
      }}
      onCopyCli={async () => {
        await copyToClipboard(cliBinary)
      }}
      onError={onError}
    />
  )
}

function DefaultSurfaceCardsFallback({ entry }: { entry: DefaultEntry }) {
  const cliBinary = appSpecs[entry.app].cliBinary
  return (
    <ProfileDetailSurfaceCardsFallback
      app={entry.app}
      surfaces={entry.surfaces}
      cliCommandLabel={<code className="font-mono">{cliBinary}</code>}
      onLaunchGui={async () => {}}
      onCopyCli={async () => {}}
    />
  )
}
