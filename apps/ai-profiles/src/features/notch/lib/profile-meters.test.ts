import type { QuotaUsage } from '@/lib/types'

import { describe, expect, it } from 'vitest'

import { elapsedFraction, formatResetsIn, meterLevel, profileMeters } from './profile-meters'

const now = new Date('2026-10-13T00:00:00Z')

function quota(overrides: Partial<QuotaUsage>): QuotaUsage {
  return { primary: null, secondary: null, scopedWeekly: [], ...overrides }
}

describe('profileMeters', () => {
  it('reads Claude’s session from the primary window and its week from the secondary', () => {
    const meters = profileMeters(
      'claude',
      quota({
        primary: { utilization: 12, resetsAt: '2026-10-13T02:30:00Z' },
        secondary: { utilization: 32, resetsAt: '2026-10-17T23:00:00Z' },
      }),
      now,
    )
    expect(meters.session).toMatchObject({ label: '5h', percent: 12 })
    expect(meters.session?.elapsed).toBeCloseTo(0.5)
    expect(meters.weekly).toMatchObject({ label: 'Week', percent: 32, resetsAt: new Date('2026-10-17T23:00:00Z') })
  })

  it('tells Codex’s windows apart by their length, whichever slot they are in', () => {
    const meters = profileMeters(
      'codex',
      quota({
        primary: { utilization: 60, resetsAt: '2026-10-19T00:00:00Z', windowDurationMins: 10080 },
        secondary: { utilization: 5, resetsAt: '2026-10-13T05:00:00Z', windowDurationMins: 300 },
      }),
      now,
    )
    expect(meters.weekly?.percent).toBe(60)
    expect(meters.session).toMatchObject({ label: '5h', percent: 5 })
  })

  it('has no weekly bar when no window lasts a week', () => {
    const meters = profileMeters(
      'codex',
      quota({ primary: { utilization: 5, resetsAt: null, windowDurationMins: 300 } }),
      now,
    )
    expect(meters.weekly).toBeNull()
    expect(meters.session?.elapsed).toBeNull()
  })
})

describe('elapsedFraction', () => {
  it('is the share of a seven-day week gone before the reset', () => {
    expect(elapsedFraction(new Date('2026-10-16T12:00:00Z'), now)).toBeCloseTo(0.5)
    expect(elapsedFraction(new Date('2026-10-20T00:00:00Z'), now)).toBe(0)
    expect(elapsedFraction(new Date('2026-10-12T00:00:00Z'), now)).toBe(1)
  })

  it('measures a session against its own length', () => {
    const reset = new Date(2026, 9, 18, 5)
    expect(elapsedFraction(reset, new Date(2026, 9, 18, 2, 30), 300)).toBeCloseTo(0.5)
  })
})

describe('formatResetsIn', () => {
  it('shows at most two units', () => {
    expect(formatResetsIn(new Date('2026-10-17T23:00:00Z'), now)).toBe('4d 23h')
    expect(formatResetsIn(new Date('2026-10-13T05:12:00Z'), now)).toBe('5h 12m')
    expect(formatResetsIn(new Date('2026-10-13T00:12:00Z'), now)).toBe('12m')
  })

  it('has nothing to say once the reset has passed', () => {
    expect(formatResetsIn(now, now)).toBeNull()
  })
})

describe('meterLevel', () => {
  it('warns from 80% and is full at the limit', () => {
    expect(meterLevel(null)).toBe('ok')
    expect(meterLevel(79)).toBe('ok')
    expect(meterLevel(80)).toBe('warn')
    expect(meterLevel(100)).toBe('full')
  })
})
