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

  it('returns null for a use within the last five minutes', () => {
    expect(formatLastUsed('2026-05-28T11:55:01Z')).toBeNull()
  })

  it('returns null for a timestamp slightly in the future', () => {
    expect(formatLastUsed('2026-05-28T12:00:30Z')).toBeNull()
  })

  it('returns a relative string from five minutes ago', () => {
    expect(formatLastUsed('2026-05-28T11:55:00Z')).toBe('Last used 5 minutes ago')
  })

  it('returns a relative string for a valid past timestamp', () => {
    expect(formatLastUsed('2026-05-28T11:00:00Z')).toBe('Last used about 1 hour ago')
  })
})
