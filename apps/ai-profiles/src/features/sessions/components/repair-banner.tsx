import { useState } from 'react'

import { StatusDot } from '@/design'

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
   * How many of its sessions need repair.
   */
  repairCount: number
}

/**
 * The banner's Repair button: a small bordered button, raised off the card.
 */
const repairButtonClasses =
  'inline-flex h-6 shrink-0 cursor-pointer items-center rounded-md border border-border bg-white px-2.5 text-[11.5px] font-medium text-ink shadow-[0_1px_2px_rgba(0,0,0,0.06)] outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:border-border-strong focus-visible:ring-2 focus-visible:ring-orange/40 dark:bg-cream-2 dark:hover:bg-white/[0.09]'

/**
 * Heads the Active list while some of the profile's sessions need repair:
 * sessions its desktop app started before the profile had its own folder,
 * which the app now opens empty. Repair asks to confirm first. Nothing shows
 * while no session needs it.
 */
export function RepairBanner({ profileId, profileLabel, repairCount }: Props) {
  const [confirming, setConfirming] = useState(false)

  if (repairCount === 0) {
    return null
  }
  const verb = repairCount === 1 ? 'needs' : 'need'
  return (
    <div className="mx-[13px] mb-[9px] flex shrink-0 items-center justify-between gap-3 rounded-lg border border-amber/25 bg-amber/[0.06] py-1 pr-1 pl-2.5">
      <p className="flex min-w-0 items-center gap-2 text-[12px] text-ink-soft">
        <StatusDot tone="warning" className="shrink-0" />
        {sessionCount(repairCount)} {verb} fixing
      </p>
      <button type="button" className={repairButtonClasses} onClick={() => setConfirming(true)}>
        Repair
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
