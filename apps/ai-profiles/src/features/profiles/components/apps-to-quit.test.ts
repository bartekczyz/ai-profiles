import { describe, expect, it } from 'vitest'

import { appsToQuitLabel, appsToQuitNote } from './apps-to-quit'

describe('apps to quit', () => {
  it('names one app or several', () => {
    expect(appsToQuitLabel([{ profileId: 'a', label: 'Work' }])).toBe('Claude (Work)')
    expect(
      appsToQuitLabel([
        { profileId: 'a', label: 'Work' },
        { profileId: 'default:claude', label: 'Default' },
      ]),
    ).toBe('Claude (Work) and Claude (Default)')
  })

  it('agrees in number', () => {
    expect(appsToQuitNote([{ profileId: 'a', label: 'Work' }])).toMatch(
      /^Claude \(Work\) will quit first, since it has .* keeps .* Its other sessions .* open it again\.$/,
    )
    expect(
      appsToQuitNote([
        { profileId: 'a', label: 'Work' },
        { profileId: 'b', label: 'Home' },
      ]),
    ).toMatch(/they have .* keep .* Their other sessions .* open them again\.$/)
  })
})
