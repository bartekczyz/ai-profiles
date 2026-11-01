import type { AppId } from '@/lib/app-registry'
import type { ProfileUsage } from '@/lib/types'

const minuteMs = 60 * 1000

/**
 * Claude reports no window lengths; its weekly limit is the long one.
 */
const claudeWeekMins = 10080

/**
 * One stretch between two resets of a limit, `to` excluded.
 */
export type LimitCycle = {
  /**
   * When the cycle started.
   */
  from: Date
  /**
   * When it resets, or reset: for the running cycle, still ahead.
   */
  to: Date
}

/**
 * The cycle running at `now` and the one before it, of the profile's longest
 * limit: the weekly one, or a free plan's only, monthly one. Null when the
 * usage doesn't say when that limit resets.
 *
 * Providers report only the next reset, so the start is the reset less the
 * window's length, and the cycle before is assumed to be as long and to end
 * where this one starts. Times are cut to the minute, as a provider's reset
 * time can move by moments between fetches, which would read the cost anew.
 */
export function limitCycles(
  app: AppId,
  quota: ProfileUsage['quota'],
  now: Date,
): { current: LimitCycle; previous: LimitCycle } | null {
  const longest = longestWindow(app, quota)
  if (!longest) {
    return null
  }
  const length = longest.minutes * minuteMs
  const reported = Math.floor(longest.resetsAt / minuteMs) * minuteMs
  // A restored snapshot's reset may have passed: its cycles have rolled on.
  const passed = Math.max(0, Math.floor((now.getTime() - reported) / length) + 1)
  const to = reported + passed * length
  return {
    current: { from: new Date(to - length), to: new Date(to) },
    previous: { from: new Date(to - 2 * length), to: new Date(to - length) },
  }
}

type KnownWindow = { resetsAt: number; minutes: number }

function longestWindow(app: AppId, quota: ProfileUsage['quota']): KnownWindow | null {
  const windows =
    app === 'claude'
      ? [knownWindow(quota?.secondary?.resetsAt, claudeWeekMins)]
      : [quota?.primary, quota?.secondary].map((window) => knownWindow(window?.resetsAt, window?.windowDurationMins))
  return windows.reduce((longest, each) => (isLonger(each, longest) ? each : longest), null)
}

function isLonger(window: KnownWindow | null, than: KnownWindow | null): boolean {
  return window !== null && (than === null || window.minutes > than.minutes)
}

/**
 * A window that says both when it resets and how long it is.
 */
function knownWindow(resetsAt: string | null | undefined, minutes: number | null | undefined): KnownWindow | null {
  if (!resetsAt || !minutes) {
    return null
  }
  const time = new Date(resetsAt).getTime()
  return Number.isNaN(time) ? null : { resetsAt: time, minutes }
}
