import type { ProfileUsage } from '@/lib/types'

import { describe, expect, it } from 'vitest'

import { narrowProfileUsage } from './use-profile-usage'

describe('narrowProfileUsage', () => {
  it('returns the input when it already matches the expected shape', () => {
    const input: ProfileUsage = {
      quota: {
        fiveHour: { utilization: 0.5, resetsAt: '2099-01-01T00:00:00Z' },
        sevenDay: null,
        sevenDaySonnet: null,
      },
      quotaError: null,
      fetchedAt: '2099-01-01T00:00:00Z',
    }
    expect(narrowProfileUsage(input)).toEqual(input)
  })

  it('falls back to safe empty when input is not an object', () => {
    const result = narrowProfileUsage('garbage')
    expect(result.quota).toBeNull()
    expect(result.quotaError).toBe('unknown')
  })

  it('coerces a NaN utilization to null', () => {
    const result = narrowProfileUsage({
      quota: { fiveHour: { utilization: Number.NaN, resetsAt: null }, sevenDay: null, sevenDaySonnet: null },
      quotaError: null,
      fetchedAt: 'x',
    })
    expect(result.quota?.fiveHour?.utilization).toBeNull()
  })

  it('treats unknown quotaError values as "unknown"', () => {
    const result = narrowProfileUsage({
      quota: null,
      quotaError: 'something_new',
      fetchedAt: 'x',
    })
    expect(result.quotaError).toBe('unknown')
  })

  it('preserves utilization values above 100 (over-limit users)', () => {
    const result = narrowProfileUsage({
      quota: { fiveHour: { utilization: 105, resetsAt: null }, sevenDay: null, sevenDaySonnet: null },
      quotaError: null,
      fetchedAt: 'x',
    })
    expect(result.quota?.fiveHour?.utilization).toBe(105)
  })

  it('drops negative utilization to null', () => {
    const result = narrowProfileUsage({
      quota: { fiveHour: { utilization: -1, resetsAt: null }, sevenDay: null, sevenDaySonnet: null },
      quotaError: null,
      fetchedAt: 'x',
    })
    expect(result.quota?.fiveHour?.utilization).toBeNull()
  })
})
