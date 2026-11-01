import type { ProfileUsage } from '@/lib/types'

import { describe, expect, it } from 'vitest'

import { limitCycles } from './limit-cycle'

const now = new Date('2026-11-01T12:00:00Z')
const day = 1440

function quota(fields: Partial<NonNullable<ProfileUsage['quota']>>): ProfileUsage['quota'] {
  return { primary: null, secondary: null, scopedWeekly: [], ...fields } as ProfileUsage['quota']
}

describe('limitCycles', () => {
  it("takes a week back from Claude's weekly reset, which reports no length", () => {
    const usage = quota({
      primary: { utilization: 1, resetsAt: '2026-11-01T14:00:00Z' },
      secondary: { utilization: 1, resetsAt: '2026-11-04T02:00:00Z' },
    })
    expect(limitCycles('claude', usage, now)).toEqual({
      current: { from: new Date('2026-10-28T02:00:00Z'), to: new Date('2026-11-04T02:00:00Z') },
      previous: { from: new Date('2026-10-21T02:00:00Z'), to: new Date('2026-10-28T02:00:00Z') },
    })
  })

  it("uses a free ChatGPT plan's only window, a month in the primary slot", () => {
    const usage = quota({
      primary: { utilization: 1, resetsAt: '2026-11-20T00:00:00Z', windowDurationMins: 30 * day },
    })
    expect(limitCycles('codex', usage, now)?.current.from).toEqual(new Date('2026-10-21T00:00:00Z'))
    expect(limitCycles('codex', usage, now)?.previous.from).toEqual(new Date('2026-09-21T00:00:00Z'))
  })

  it('picks the longer of two Codex windows, whichever slot it is in', () => {
    const usage = quota({
      primary: { utilization: 1, resetsAt: '2026-11-04T00:00:00Z', windowDurationMins: 7 * day },
      secondary: { utilization: 1, resetsAt: '2026-11-01T14:00:00Z', windowDurationMins: 300 },
    })
    expect(limitCycles('codex', usage, now)?.current.to).toEqual(new Date('2026-11-04T00:00:00Z'))
  })

  it('rolls a reset that has passed on to the cycle running now', () => {
    const usage = quota({ secondary: { utilization: 1, resetsAt: '2026-10-20T00:00:00Z' } })
    expect(limitCycles('claude', usage, now)?.current).toEqual({
      from: new Date('2026-10-27T00:00:00Z'),
      to: new Date('2026-11-03T00:00:00Z'),
    })
  })

  it('stays put when a reset time moves by seconds', () => {
    const at = (resetsAt: string) => limitCycles('claude', quota({ secondary: { utilization: 1, resetsAt } }), now)
    expect(at('2026-11-04T02:00:10Z')).toEqual(at('2026-11-04T02:00:40Z'))
  })

  it('has no cycles without a reset time or a known length', () => {
    expect(limitCycles('claude', quota({ secondary: { utilization: 1, resetsAt: null } }), now)).toBeNull()
    expect(
      limitCycles('codex', quota({ primary: { utilization: 1, resetsAt: '2026-11-04T00:00:00Z' } }), now),
    ).toBeNull()
    expect(limitCycles('claude', null, now)).toBeNull()
  })
})
