import type { ChangelogRelease } from './changelog'

import { describe, expect, it } from 'vitest'

import { resolveWhatsNew } from './resolve-whats-new'

function withNotes(version: string): ChangelogRelease {
  return { version, date: null, sections: [{ title: 'Added', items: [{ text: version }] }] }
}

function withoutNotes(version: string): ChangelogRelease {
  return { version, date: null, sections: [] }
}

const releases = [withNotes('1.2.0'), withNotes('1.1.0'), withNotes('1.0.0')]

function versionsOf(resolution: { releases: Array<ChangelogRelease> }): Array<string> {
  return resolution.releases.map((release) => release.version)
}

describe('resolveWhatsNew', () => {
  it.each([
    ['no stored version (fresh install)', null, '1.2.0'],
    ['unparseable stored version', 'garbage', '1.2.0'],
    ['same version', '1.2.0', '1.2.0'],
    ['a downgrade', '1.2.0', '1.0.0'],
  ])('is not an upgrade on %s and offers only the current release', (_label, lastSeen, current) => {
    const resolution = resolveWhatsNew({ current, lastSeen, releases })

    expect(resolution.upgraded).toBe(false)
    expect(versionsOf(resolution)).toEqual([current])
  })

  it('is an upgrade and lists every skipped release, newest first', () => {
    const resolution = resolveWhatsNew({ current: '1.2.0', lastSeen: '1.0.0', releases })

    expect(resolution.upgraded).toBe(true)
    expect(versionsOf(resolution)).toEqual(['1.2.0', '1.1.0'])
  })

  it('is not an upgrade when the versions in between have no notes', () => {
    const resolution = resolveWhatsNew({
      current: '1.1.0',
      lastSeen: '1.0.0',
      releases: [withNotes('1.0.0'), withoutNotes('1.1.0')],
    })

    expect(resolution.upgraded).toBe(false)
    expect(versionsOf(resolution)).toEqual([])
  })

  it('offers nothing when the current version has no changelog entry', () => {
    const resolution = resolveWhatsNew({ current: '9.9.9', lastSeen: null, releases })

    expect(resolution.upgraded).toBe(false)
    expect(versionsOf(resolution)).toEqual([])
  })
})
