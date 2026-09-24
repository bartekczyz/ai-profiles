import { useState } from 'react'

import { X } from 'lucide-react'

import { StatusDot } from '@/design'
import { useAppState } from '@/lib/app-state/use-app-state'

import { repairOfferShown } from '../lib/repair-offer'
import { sessionCount } from '../lib/session-count'
import { RepairSessionsDialog } from './repair-sessions-dialog'

type Props = {
  /**
   * The profile whose sessions are listed — a managed profile's id, or
   * `default:<app>`.
   */
  profileId: string
  /**
   * The profile's name.
   */
  profileLabel: string
  /**
   * The ids of its sessions that need repair.
   */
  repairSessionIds: Array<string>
}

/**
 * The banner's Repair button: a small bordered button, raised off the card.
 */
const repairButtonClasses =
  'inline-flex h-6 shrink-0 cursor-pointer items-center rounded-md border border-border bg-white px-2.5 text-[11.5px] font-medium text-ink shadow-[0_1px_2px_rgba(0,0,0,0.06)] outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:border-border-strong focus-visible:ring-2 focus-visible:ring-orange/40 dark:bg-cream-2 dark:hover:bg-white/[0.09]'

/**
 * The banner's Not now button: a quiet icon that fades in on hover.
 */
const dismissButtonClasses =
  'grid h-6 w-6 shrink-0 cursor-pointer place-items-center rounded-md text-muted-strong outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:bg-ink/[0.06] hover:text-ink focus-visible:ring-2 focus-visible:ring-orange/40'

/**
 * Heads the Active list while some of the profile's sessions need repair:
 * sessions its desktop app started before the profile had its own folder,
 * which the app now opens empty. Nothing shows while no session needs it,
 * and then nothing waits on what was dismissed.
 */
export function RepairBanner({ profileId, profileLabel, repairSessionIds }: Props) {
  if (repairSessionIds.length === 0) {
    return null
  }
  return <RepairOffer profileId={profileId} profileLabel={profileLabel} repairSessionIds={repairSessionIds} />
}

/**
 * The offer to repair: Repair asks to confirm first; Not now puts it away for
 * the sessions that need repair now, and it comes back when another one does.
 */
function RepairOffer({ profileId, profileLabel, repairSessionIds }: Props) {
  const [confirming, setConfirming] = useState(false)
  const appState = useAppState()
  const dismissed = appState.state.dismissedRepairSessions[profileId] ?? []

  if (!repairOfferShown(repairSessionIds, dismissed)) {
    return null
  }
  const repairCount = repairSessionIds.length
  const verb = repairCount === 1 ? 'needs' : 'need'
  return (
    <div className="mx-[13px] mb-[9px] flex shrink-0 items-center gap-1 rounded-lg border border-amber/25 bg-amber/[0.06] py-1 pr-1 pl-2.5">
      <p className="flex min-w-0 flex-1 items-center gap-2 text-[12px] text-ink-soft">
        <StatusDot tone="warning" className="shrink-0" />
        {sessionCount(repairCount)} {verb} fixing
      </p>
      <button type="button" className={repairButtonClasses} onClick={() => setConfirming(true)}>
        Repair
      </button>
      <button
        type="button"
        aria-label="Not now"
        title="Not now"
        className={dismissButtonClasses}
        onClick={() => {
          void appState.update({ dismissedRepair: { profileId, sessionIds: repairSessionIds } })
        }}
      >
        <X aria-hidden className="h-3.5 w-3.5" />
      </button>
      {confirming ? (
        <RepairSessionsDialog
          profileId={profileId}
          profileLabel={profileLabel}
          repairCount={repairCount}
          onClose={() => setConfirming(false)}
        />
      ) : null}
    </div>
  )
}
