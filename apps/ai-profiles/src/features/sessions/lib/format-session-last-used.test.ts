import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { formatSessionLastUsed } from './format-session-last-used'

describe('formatSessionLastUsed', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-09-23T12:00:00Z'))
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('describes how long ago the session was used', () => {
    expect(formatSessionLastUsed('2026-09-23T11:48:00Z')).toBe('12 minutes ago')
    expect(formatSessionLastUsed('2026-09-21T12:00:00Z')).toBe('2 days ago')
  })

  it('returns nothing for a timestamp it cannot read', () => {
    expect(formatSessionLastUsed('not a date')).toBeNull()
  })
})
