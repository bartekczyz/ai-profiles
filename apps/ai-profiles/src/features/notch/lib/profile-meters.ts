import type { AppId } from '@/lib/app-registry'
import type { QuotaUsage, UsageWindow } from '@/lib/types'

import { codexWindowLabel } from '@/features/profiles/components/profile-detail-usage-card'

const weekMinutes = 7 * 24 * 60
const claudeSessionMinutes = 5 * 60

/** A profile's working week: every day, or Monday to Friday. */
export type WorkingWeek = 5 | 7

/** One of a profile's limits, as a bar draws it. */
export type LimitMeter = {
  /** The bar's caption: `5h`, `Week`. */
  label: string
  /** Share of the limit used, 0-100 and beyond, or null when unknown. */
  percent: number | null
  /** Share of the window gone, 0-1: where an even pace would be by now. */
  elapsed: number | null
  resetsAt: Date | null
}

/** What the notch HUD draws for one profile: the session bar over the weekly one. */
export type ProfileMeters = {
  session: LimitMeter | null
  weekly: LimitMeter | null
}

type LimitWindow = { window: UsageWindow; minutes: number; label: string }

/**
 * Pure: a profile's two windows. Claude reports the session as `primary`
 * and the week as `secondary`; Codex windows are positions, told apart by
 * their length.
 */
function limitWindows(app: AppId, quota: QuotaUsage): { session: LimitWindow | null; weekly: LimitWindow | null } {
  if (app === 'codex') {
    const windows = [quota.primary, quota.secondary].filter((window) => window !== null)
    const weekly = windows.find((window) => window.windowDurationMins === weekMinutes)
    const session = windows.find((window) => window.windowDurationMins && window.windowDurationMins !== weekMinutes)
    return {
      weekly: weekly ? { window: weekly, minutes: weekMinutes, label: 'Week' } : null,
      session: session?.windowDurationMins
        ? {
            window: session,
            minutes: session.windowDurationMins,
            label: codexWindowLabel(session.windowDurationMins).shortLabel,
          }
        : null,
    }
  }
  return {
    weekly: quota.secondary ? { window: quota.secondary, minutes: weekMinutes, label: 'Week' } : null,
    session: quota.primary ? { window: quota.primary, minutes: claudeSessionMinutes, label: '5h' } : null,
  }
}

function parseDate(value: string | null): Date | null {
  if (!value) {
    return null
  }
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? null : date
}

/** Pure: milliseconds of `from..to` on Monday to Friday, in local time. */
function weekdayMs(from: Date, to: Date): number {
  let total = 0
  let cursor = from
  while (cursor < to) {
    const midnight = new Date(cursor.getFullYear(), cursor.getMonth(), cursor.getDate() + 1)
    const end = midnight < to ? midnight : to
    const day = cursor.getDay()
    if (day >= 1 && day <= 5) {
      total += end.getTime() - cursor.getTime()
    }
    cursor = end
  }
  return total
}

/**
 * Pure: the share of a window gone by `now`, 0-1, from when it resets. A
 * tick at this point on the bar says whether usage is ahead of an even pace.
 * A five-day week counts only weekday time, so the tick holds still over the
 * weekend; a session always counts the clock.
 */
export function elapsedFraction(resetsAt: Date, now: Date, minutes = weekMinutes, week: WorkingWeek = 7): number {
  const start = new Date(resetsAt.getTime() - minutes * 60_000)
  if (week === 5 && minutes === weekMinutes) {
    const total = weekdayMs(start, resetsAt)
    if (total > 0) {
      const clamped = now < start ? start : now > resetsAt ? resetsAt : now
      return weekdayMs(start, clamped) / total
    }
  }
  return Math.min(1, Math.max(0, (now.getTime() - start.getTime()) / (minutes * 60_000)))
}

function limitMeter(limit: LimitWindow | null, now: Date, week: WorkingWeek): LimitMeter | null {
  if (!limit) {
    return null
  }
  const resetsAt = parseDate(limit.window.resetsAt)
  return {
    label: limit.label,
    percent: limit.window.utilization,
    elapsed: resetsAt ? elapsedFraction(resetsAt, now, limit.minutes, week) : null,
    resetsAt,
  }
}

/** Pure: the notch HUD's reading of one profile's quota. */
export function profileMeters(app: AppId, quota: QuotaUsage, now: Date, week: WorkingWeek = 7): ProfileMeters {
  const windows = limitWindows(app, quota)
  return { session: limitMeter(windows.session, now, week), weekly: limitMeter(windows.weekly, now, week) }
}

/**
 * Pure: the time to a reset in at most two units, `4d 23h`, `5h 12m`, `12m`.
 * Null once the reset has passed.
 */
export function formatResetsIn(resetsAt: Date, now: Date): string | null {
  const minutes = Math.ceil((resetsAt.getTime() - now.getTime()) / 60_000)
  if (minutes <= 0) {
    return null
  }
  const days = Math.floor(minutes / (24 * 60))
  const hours = Math.floor((minutes % (24 * 60)) / 60)
  const rest = minutes % 60
  if (days > 0) {
    return `${days}d ${hours}h`
  }
  if (hours > 0) {
    return `${hours}h ${rest}m`
  }
  return `${rest}m`
}

/** How pressed a limit is, for its colour. */
export type MeterLevel = 'ok' | 'warn' | 'full'

/** Pure: `warn` from 80% used, `full` at the limit. */
export function meterLevel(percent: number | null): MeterLevel {
  if (percent === null || percent < 80) {
    return 'ok'
  }
  return percent >= 100 ? 'full' : 'warn'
}
