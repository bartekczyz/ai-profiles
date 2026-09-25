import { describe, expect, it } from 'vitest'

import { shouldShowPathBanner } from './path-banner'

const now = new Date('2026-09-25T12:00:00Z').getTime()
const dayMs = 24 * 60 * 60 * 1000

const showing = {
  welcomeShown: true,
  localBinOnPath: false,
  anyCliProfile: true,
  dismissedAt: null,
  now,
}

describe('shouldShowPathBanner', () => {
  it('shows once onboarding is done, a CLI profile exists and ~/.local/bin is off PATH', () => {
    expect(shouldShowPathBanner(showing)).toBe(true)
  })

  it('stays hidden until the welcome dialog was seen', () => {
    expect(shouldShowPathBanner({ ...showing, welcomeShown: false })).toBe(false)
  })

  it('stays hidden when ~/.local/bin is already on PATH', () => {
    expect(shouldShowPathBanner({ ...showing, localBinOnPath: true })).toBe(false)
  })

  it('stays hidden when no profile has a CLI wrapper', () => {
    expect(shouldShowPathBanner({ ...showing, anyCliProfile: false })).toBe(false)
  })

  it('stays hidden for 49 days after a dismissal', () => {
    const dismissedAt = new Date(now - 48 * dayMs).toISOString()
    expect(shouldShowPathBanner({ ...showing, dismissedAt })).toBe(false)
  })

  it('comes back once the dismissal is older than 49 days', () => {
    const dismissedAt = new Date(now - 50 * dayMs).toISOString()
    expect(shouldShowPathBanner({ ...showing, dismissedAt })).toBe(true)
  })
})
