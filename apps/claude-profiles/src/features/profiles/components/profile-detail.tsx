import type { Profile, Surface } from '@/lib/types'

import { Suspense, useState } from 'react'

import { wrapperCommand } from '@/lib/app-registry'

import { useProfileLastUsed } from '../api/use-profile-last-used'
import { useProfilePaths } from '../api/use-profile-paths'
import { formatLastUsed } from './format-last-used'
import { ProfileDetailDangerLink } from './profile-detail-danger-link'
import { ProfileDetailHeader, ProfileSwatch } from './profile-detail-header'
import { ProfileDetailRecentActivity } from './profile-detail-recent-activity'
import { ProfileDetailShell } from './profile-detail-shell'
import { ProfileDetailSurfaceCards, ProfileDetailSurfaceCardsFallback } from './profile-detail-surface-cards'
import { ProfileDetailUsageCard } from './profile-detail-usage-card'

type Props = {
  profile: Profile
  onEdit: () => void
  onDelete: () => void
  // Kept for the More menu in a later phase.
  onToggle: (surface: Surface, enabled: boolean) => Promise<void>
}

export function ProfileDetail({ profile, onEdit, onDelete }: Props) {
  const [actionError, setActionError] = useState<string | null>(null)

  return (
    <ProfileDetailShell>
      <ProfileDetailHeader
        name={profile.name}
        swatch={<ProfileSwatch color={profile.color} />}
        subline={
          <>
            <span>{profile.app}</span>
            <span className="mx-2 text-border">·</span>
            <span className="text-muted-strong">{formatLastUsed(profile.lastUsedAt)}</span>
          </>
        }
        onEdit={onEdit}
      />

      <ProfileDetailUsageCard app={profile.app} profileId={profile.id} cliEnabled={profile.surfaces.cli} />

      <div className="mb-6 grid grid-cols-1 gap-3.5 lg:grid-cols-2">
        <Suspense key={profile.id} fallback={<ManagedSurfaceCardsFallback profile={profile} />}>
          <ManagedSurfaceCards profile={profile} onError={setActionError} />
        </Suspense>
      </div>

      {actionError ? (
        <p role="alert" className="mb-4 text-meta text-red">
          {actionError}
        </p>
      ) : null}

      <ProfileDetailRecentActivity profileId={profile.id} />
      <ProfileDetailDangerLink onDelete={onDelete} />
    </ProfileDetailShell>
  )
}

type ManagedSurfaceCardsProps = {
  profile: Profile
  onError: (message: string | null) => void
}

function ManagedSurfaceCards({ profile, onError }: ManagedSurfaceCardsProps) {
  const paths = useProfilePaths(profile.id)
  const lastUsed = useProfileLastUsed()
  const command = wrapperCommand(profile.app, profile.slug)
  return (
    <ProfileDetailSurfaceCards
      app={profile.app}
      paths={paths}
      surfaces={profile.surfaces}
      cliCommandLabel={<code className="font-mono">{command}</code>}
      onLaunchGui={() => lastUsed.launchDesktop(profile.id)}
      onCopyCli={() => lastUsed.copyCli({ profileId: profile.id, command })}
      onError={onError}
    />
  )
}

function ManagedSurfaceCardsFallback({ profile }: { profile: Profile }) {
  const lastUsed = useProfileLastUsed()
  const command = wrapperCommand(profile.app, profile.slug)
  return (
    <ProfileDetailSurfaceCardsFallback
      app={profile.app}
      surfaces={profile.surfaces}
      cliCommandLabel={<code className="font-mono">{command}</code>}
      onLaunchGui={() => lastUsed.launchDesktop(profile.id)}
      onCopyCli={() => lastUsed.copyCli({ profileId: profile.id, command })}
    />
  )
}
