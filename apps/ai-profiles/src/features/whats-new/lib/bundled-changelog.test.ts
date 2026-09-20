import { describe, expect, it } from 'vitest'

import packageJson from '../../../../package.json'
import { bundledReleases } from './bundled-changelog'

// Guards against release-please's changelog format (or the version sync) drifting: if this fails
// in CI the bundled notes would silently be empty for users.
describe('bundledReleases', () => {
  it('parses the real changelog with the newest release matching the app version', () => {
    expect(bundledReleases.length).toBeGreaterThan(0)
    expect(bundledReleases[0].version).toBe(packageJson.version)
  })

  it('gives the newest release at least one item', () => {
    expect(bundledReleases[0].sections.length).toBeGreaterThan(0)
  })
})
