import type { Profile } from '@/lib/types'

import { describe, expect, it } from 'vitest'

import { planProfileEdit } from './plan-profile-edit'

const profile: Profile = {
  id: 'p1',
  app: 'claude',
  name: 'Work',
  slug: 'work',
  color: '#AABBCC',
  createdAt: '2026-01-01T00:00:00Z',
  surfaces: { gui: true, cli: false },
  distinctDockIcon: false,
  lastUsedAt: null,
}

const unchanged = {
  name: 'Work',
  color: '#AABBCC',
  surfaces: { gui: true, cli: false },
  distinctDockIcon: false,
}

describe('planProfileEdit', () => {
  it('plans nothing when the form matches the profile', () => {
    expect(planProfileEdit(profile, unchanged)).toEqual({ patch: null, toggles: [] })
  })

  it('treats a color differing only in case as unchanged', () => {
    expect(planProfileEdit(profile, { ...unchanged, color: '#aabbcc' })).toEqual({ patch: null, toggles: [] })
  })

  it('patches name and color together when the name changes, leaving the Dock icon out', () => {
    expect(planProfileEdit(profile, { ...unchanged, name: 'Home' }).patch).toEqual({
      name: 'Home',
      color: '#AABBCC',
    })
  })

  it('patches name and color together when the color changes', () => {
    expect(planProfileEdit(profile, { ...unchanged, color: '#112233' }).patch).toEqual({
      name: 'Work',
      color: '#112233',
    })
  })

  it('includes the Dock icon in the patch only when it changes', () => {
    expect(planProfileEdit(profile, { ...unchanged, distinctDockIcon: true }).patch).toEqual({
      name: 'Work',
      color: '#AABBCC',
      distinctDockIcon: true,
    })
  })

  it('toggles each surface that changed, gui before cli', () => {
    expect(planProfileEdit(profile, { ...unchanged, surfaces: { gui: false, cli: true } }).toggles).toEqual([
      { surface: 'gui', enabled: false },
      { surface: 'cli', enabled: true },
    ])
  })

  it('toggles only the surface that changed, without patching', () => {
    expect(planProfileEdit(profile, { ...unchanged, surfaces: { gui: true, cli: true } })).toEqual({
      patch: null,
      toggles: [{ surface: 'cli', enabled: true }],
    })
  })
})
