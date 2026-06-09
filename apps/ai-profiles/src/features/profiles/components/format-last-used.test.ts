import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { formatLastUsed } from './format-last-used'

describe('formatLastUsed', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-05-28T12:00:00Z'))
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('returns "Never used" when the timestamp is null', () => {
    expect(formatLastUsed(null)).toBe('Never used')
  })

  it('returns "Never used" when the timestamp is not a valid date string', () => {
    expect(formatLastUsed('not-a-date')).toBe('Never used')
  })

  it('returns a relative string for a valid past timestamp', () => {
    const result = formatLastUsed('2026-05-28T11:00:00Z')
    expect(result.startsWith('Last used ')).toBe(true)
    expect(result).toContain('hour')
  })
})
