import { useState } from 'react'

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
 * The banner's Repair button: a quiet bordered pill that fits the row.
 */
const repairButtonClasses =
  'inline-flex h-7 shrink-0 cursor-pointer items-center rounded-[7px] border border-border bg-white/60 px-2.5 text-[12px] text-ink-soft outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:border-border-strong hover:bg-white focus-visible:ring-2 focus-visible:ring-orange/40 dark:bg-white/[0.05] dark:hover:bg-white/[0.09]'

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
    <div className="mb-2 flex items-center justify-between gap-3 rounded-[10px] border border-border-soft px-[13px] py-[9px]">
      <p className="min-w-0 text-[12px] text-ink-soft">
        {sessionCount(repairCount)} {verb} repair
        <span aria-hidden className="text-muted">
          {' '}
          ·
        </span>
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
