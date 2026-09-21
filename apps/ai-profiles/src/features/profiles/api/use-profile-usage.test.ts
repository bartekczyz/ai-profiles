import type { ProfileUsage } from '@/lib/types'

import { QueryClient } from '@tanstack/react-query'
import { describe, expect, it } from 'vitest'

import { queryKeys } from '@/lib/query/keys'

import { ensureUsable, narrowProfileUsage, UsageUnavailableError } from './use-profile-usage'

describe('narrowProfileUsage', () => {
  it('preserves reset counts and nullable expiry details', () => {
    const resets = { availableCount: 2, credits: [{ title: 'Full reset', status: 'available', expiresAt: null }] }
    const result = narrowProfileUsage({ quota: { rateLimitResetCredits: resets } })
    expect(result.quota).toHaveProperty('rateLimitResetCredits', resets)
  })

  it.each([10080, 300, 60])('preserves quota duration %s across IPC narrowing', (duration) => {
    const result = narrowProfileUsage({
      quota: {
        primary: { utilization: 14, resetsAt: null, windowDurationMins: duration },
      },
    })
    expect(result.quota?.primary).toHaveProperty('windowDurationMins', duration)
  })

  it('returns the input when it already matches the expected shape', () => {
    const input: ProfileUsage = {
      quota: {
        primary: { utilization: 0.5, resetsAt: '2099-01-01T00:00:00Z' },
        secondary: null,
        scopedWeekly: [],
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
      quota: { primary: { utilization: Number.NaN, resetsAt: null }, secondary: null, scopedWeekly: [] },
      quotaError: null,
      fetchedAt: 'x',
    })
    expect(result.quota?.primary?.utilization).toBeNull()
  })

  it('preserves the rate_limited quotaError', () => {
    const result = narrowProfileUsage({
      quota: null,
      quotaError: 'rate_limited',
      fetchedAt: 'x',
    })
    expect(result.quotaError).toBe('rate_limited')
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
      quota: { primary: { utilization: 105, resetsAt: null }, secondary: null, scopedWeekly: [] },
      quotaError: null,
      fetchedAt: 'x',
    })
    expect(result.quota?.primary?.utilization).toBe(105)
  })

  it('drops negative utilization to null', () => {
    const result = narrowProfileUsage({
      quota: { primary: { utilization: -1, resetsAt: null }, secondary: null, scopedWeekly: [] },
      quotaError: null,
      fetchedAt: 'x',
    })
    expect(result.quota?.primary?.utilization).toBeNull()
  })

  it('keeps the server-supplied window label', () => {
    const result = narrowProfileUsage({
      quota: {
        primary: null,
        secondary: null,
        scopedWeekly: [{ utilization: 19, resetsAt: null, label: 'Fable' }],
      },
      quotaError: null,
      fetchedAt: 'x',
    })
    expect(result.quota?.scopedWeekly[0]).toHaveProperty('label', 'Fable')
  })

  it('drops a blank or non-string window label', () => {
    const result = narrowProfileUsage({
      quota: {
        primary: null,
        secondary: null,
        scopedWeekly: [
          { utilization: 1, resetsAt: null, label: '   ' },
          { utilization: 2, resetsAt: null, label: 7 },
        ],
      },
      quotaError: null,
      fetchedAt: 'x',
    })
    expect(result.quota?.scopedWeekly[0]).not.toHaveProperty('label')
    expect(result.quota?.scopedWeekly[1]).not.toHaveProperty('label')
  })

  it('drops unusable entries from scopedWeekly instead of the whole list', () => {
    const result = narrowProfileUsage({
      quota: {
        primary: null,
        secondary: null,
        scopedWeekly: [{ utilization: 19, resetsAt: null, label: 'Fable' }, 'garbage', null],
      },
      quotaError: null,
      fetchedAt: 'x',
    })
    expect(result.quota?.scopedWeekly).toHaveLength(1)
  })

  it('falls back to an empty scopedWeekly when the field is missing or not a list', () => {
    const result = narrowProfileUsage({
      quota: { primary: null, secondary: null, scopedWeekly: 'nope' },
      quotaError: null,
      fetchedAt: 'x',
    })
    expect(result.quota?.scopedWeekly).toEqual([])
  })

  it('keeps a well-formed spend block', () => {
    const result = narrowProfileUsage({
      quota: {
        primary: null,
        secondary: null,
        scopedWeekly: [],
        spend: { usedMinor: 7788, limitMinor: 30000, currency: 'GBP', exponent: 2, percent: 26 },
      },
      quotaError: null,
      fetchedAt: 'x',
    })
    expect(result.quota?.spend).toEqual({
      usedMinor: 7788,
      limitMinor: 30000,
      currency: 'GBP',
      exponent: 2,
      percent: 26,
    })
  })

  it.each([
    ['a fractional amount', { usedMinor: 77.5, limitMinor: 30000, currency: 'GBP', exponent: 2, percent: 26 }],
    ['a missing currency', { usedMinor: 7788, limitMinor: 30000, exponent: 2, percent: 26 }],
    ['a NaN amount', { usedMinor: Number.NaN, limitMinor: 30000, currency: 'GBP', exponent: 2, percent: 26 }],
  ])('drops spend with %s rather than rendering a nonsense price', (_label, spend) => {
    const result = narrowProfileUsage({
      quota: { primary: null, secondary: null, scopedWeekly: [], spend },
      quotaError: null,
      fetchedAt: 'x',
    })
    expect(result.quota?.spend).toBeUndefined()
  })

  it('clamps an out-of-range exponent to two decimal places', () => {
    // Intl throws outside 0..20 fraction digits; a bad exponent must not
    // take the whole card down.
    const result = narrowProfileUsage({
      quota: {
        primary: null,
        secondary: null,
        scopedWeekly: [],
        spend: { usedMinor: 7788, limitMinor: null, currency: 'GBP', exponent: 99, percent: null },
      },
      quotaError: null,
      fetchedAt: 'x',
    })
    expect(result.quota?.spend?.exponent).toBe(2)
  })
})

describe('ensureUsable', () => {
  it('returns the snapshot unchanged when it has a quota and no error', () => {
    const usage: ProfileUsage = {
      quota: { primary: { utilization: 50, resetsAt: null }, secondary: null, scopedWeekly: [] },
      quotaError: null,
      fetchedAt: 'x',
    }
    expect(ensureUsable(usage)).toBe(usage)
  })

  it('throws UsageUnavailableError carrying the code when a quotaError is present', () => {
    let thrown: unknown
    try {
      ensureUsable({ quota: null, quotaError: 'rate_limited', fetchedAt: 'x' })
    } catch (error) {
      thrown = error
    }
    expect(thrown).toBeInstanceOf(UsageUnavailableError)
    expect((thrown as UsageUnavailableError).code).toBe('rate_limited')
  })

  it('throws with code "unknown" when quota is null without an explicit error', () => {
    let thrown: unknown
    try {
      ensureUsable({ quota: null, quotaError: null, fetchedAt: 'x' })
    } catch (error) {
      thrown = error
    }
    expect((thrown as UsageUnavailableError).code).toBe('unknown')
  })
})

describe('profileUsage query key', () => {
  it('is not a subkey of the profiles namespace', () => {
    // Critical: profile-list mutations invalidate `['profiles']`, and
    // TanStack matches by prefix. If usage were under that prefix every
    // reorder / delete / migration would refetch every visible
    // profile's quota in parallel and trip the rate limiter. This test
    // pins the structural decision.
    expect(queryKeys.profileUsage('any')[0]).not.toBe(queryKeys.profiles.all[0])
  })

  it('survives a prefix invalidation of the profiles namespace', async () => {
    const client = new QueryClient()
    client.setQueryData(queryKeys.profileUsage('p1'), { marker: true })
    await client.invalidateQueries({ queryKey: queryKeys.profiles.all })
    const state = client.getQueryState(queryKeys.profileUsage('p1'))
    expect(state?.isInvalidated).toBe(false)
  })
})
