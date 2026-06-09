import type { Activity, ActivityKind } from '@/lib/types'

import { Suspense } from 'react'

import { formatDistanceToNow } from 'date-fns'
import { Copy, Pencil, Play, Plus, Settings2, SwatchBook, Upload } from 'lucide-react'

import { Skeleton } from '@/design'

import { useProfileActivity } from '../api/use-activity'

type Props = {
  profileId: string
}

/**
 * Recent-activity timeline. The actual list query lives in an inner
 * component so the eyebrow + container stay mounted while the activity
 * file is read; the fallback fills in three muted skeleton rows.
 */
export function ProfileDetailRecentActivity({ profileId }: Props) {
  return (
    <section className="mb-6">
      <div className="mb-2.5 font-mono text-eyebrow font-medium uppercase tracking-[0.1em] text-muted-strong">
        Recent activity
      </div>
      <Suspense key={profileId} fallback={<RecentActivityFallback />}>
        <RecentActivityList profileId={profileId} />
      </Suspense>
    </section>
  )
}

function RecentActivityList({ profileId }: Props) {
  const entries = useProfileActivity(profileId, 3)
  if (entries.length === 0) {
    return <p className="font-mono text-mono text-muted-strong">No activity yet.</p>
  }
  return (
    <ul className="m-0 flex list-none flex-col gap-0.5 p-0">
      {entries.map((activity) => (
        <ActivityRow key={`${activity.at}-${activity.kind}`} activity={activity} />
      ))}
    </ul>
  )
}

function RecentActivityFallback() {
  return (
    <ul className="m-0 flex list-none flex-col gap-0.5 p-0">
      {[0, 1, 2].map((row) => (
        <li key={row} className="grid grid-cols-[18px_1fr_auto] items-center gap-2.5 px-1 py-1.5">
          <Skeleton shape="circle" className="h-3 w-3" />
          <Skeleton shape="text" className="h-3 w-3/4" />
          <Skeleton shape="text" className="h-2.5 w-16" />
        </li>
      ))}
    </ul>
  )
}

type ActivityRowProps = {
  activity: Activity
}

const ICONS: Record<ActivityKind, typeof Play> = {
  created: Plus,
  renamed: Pencil,
  color_changed: SwatchBook,
  surface_toggled: Settings2,
  launched_gui: Play,
  copied_cli: Copy,
  imported: Upload,
}

function describe(activity: Activity): string {
  switch (activity.kind) {
    case 'created':
      return 'Created profile'
    case 'renamed': {
      const to = activity.metadata?.to
      return typeof to === 'string' ? `Renamed to "${to}"` : 'Renamed'
    }
    case 'color_changed':
      return 'Changed color'
    case 'surface_toggled': {
      const surface = activity.metadata?.surface
      const enabled = activity.metadata?.enabled
      const surfaceLabel = surface === 'gui' ? 'Desktop App' : surface === 'cli' ? 'Claude Code CLI' : 'Surface'
      return `${enabled ? 'Enabled' : 'Disabled'} ${surfaceLabel}`
    }
    case 'launched_gui':
      return 'Opened Desktop App'
    case 'copied_cli':
      return 'Copied CLI command'
    case 'imported':
      return 'Imported from existing install'
  }
}

function ActivityRow({ activity }: ActivityRowProps) {
  const Icon = ICONS[activity.kind]
  const parsed = new Date(activity.at)
  const when = Number.isNaN(parsed.getTime()) ? '' : formatDistanceToNow(parsed, { addSuffix: true })
  return (
    <li className="grid grid-cols-[18px_1fr_auto] items-center gap-2.5 px-1 py-1.5 text-body tracking-[-0.003em] text-ink-soft">
      <span aria-hidden className="grid place-items-center text-muted">
        <Icon className="h-3 w-3" strokeWidth={1.85} />
      </span>
      <span>{describe(activity)}</span>
      <span className="font-mono text-mono text-muted-strong">{when}</span>
    </li>
  )
}
