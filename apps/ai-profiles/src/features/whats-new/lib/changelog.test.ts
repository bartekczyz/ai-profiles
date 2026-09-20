import type { ChangelogRelease } from './changelog'

import { describe, expect, it } from 'vitest'

import { compareVersions, isVersion, parseChangelog, releasesSince } from './changelog'

const sampleChangelog = `# Changelog

## [1.1.0](https://github.com/bartekczyz/ai-profiles/compare/v1.0.2...v1.1.0) (2026-08-09)


### Added

* **ui:** rebuild the profile pane as grouped rows ([#37](https://github.com/bartekczyz/ai-profiles/issues/37)) ([a427b39](https://github.com/bartekczyz/ai-profiles/commit/a427b397a8aa12ed852e6b40339b5df9f86fb04b))


### Fixed

* **cli:** profile wrappers inherit skills, agents and global instructions ([#34](https://github.com/bartekczyz/ai-profiles/issues/34)) ([af64262](https://github.com/bartekczyz/ai-profiles/commit/af64262a5c352a6692b889960c715feebb1d1668))
* item without a scope ([abc1234](https://github.com/bartekczyz/ai-profiles/commit/abc1234))

## [1.0.0](https://github.com/bartekczyz/ai-profiles/compare/v0.6.0...v1.0.0) (2026-06-09)


### ⚠ BREAKING CHANGES

* the bundle identifier changes, existing installs do not auto-update

### Changed

* release 1.0.0 ([464674c](https://github.com/bartekczyz/ai-profiles/commit/464674c))

## 0.1.0

### Added

* first cut
`

describe('parseChangelog', () => {
  const releases = parseChangelog(sampleChangelog)

  it('returns releases in file order with version and date', () => {
    expect(releases.map((release) => [release.version, release.date])).toEqual([
      ['1.1.0', '2026-08-09'],
      ['1.0.0', '2026-06-09'],
      ['0.1.0', null],
    ])
  })

  it('parses sections and items, dropping scope markup and PR/commit links', () => {
    expect(releases[0].sections).toEqual([
      { title: 'Added', items: [{ scope: 'ui', text: 'rebuild the profile pane as grouped rows' }] },
      {
        title: 'Fixed',
        items: [
          { scope: 'cli', text: 'profile wrappers inherit skills, agents and global instructions' },
          { text: 'item without a scope' },
        ],
      },
    ])
  })

  it('strips the warning sign from the breaking-changes title', () => {
    expect(releases[1].sections[0]).toEqual({
      title: 'BREAKING CHANGES',
      items: [{ text: 'the bundle identifier changes, existing installs do not auto-update' }],
    })
  })

  it('joins wrapped continuation lines into the item', () => {
    const [release] = parseChangelog(
      [
        '## 1.0.0 (2026-01-01)',
        '',
        '### Fixed',
        '',
        '* **usage:** a long item',
        '  that wraps ([#1](https://x/1))',
      ].join('\n'),
    )

    expect(release.sections[0].items).toEqual([{ scope: 'usage', text: 'a long item that wraps' }])
  })

  it('does not treat a paragraph after a blank line as a continuation', () => {
    const [release] = parseChangelog(
      ['## 1.0.0 (2026-01-01)', '', '### Fixed', '', '* first item', '', 'stray paragraph'].join('\n'),
    )

    expect(release.sections[0].items).toEqual([{ text: 'first item' }])
  })

  it('drops sections without items but keeps the release', () => {
    expect(parseChangelog(['## 1.0.0 (2026-01-01)', '', '### Added', ''].join('\n'))).toEqual([
      { version: '1.0.0', date: '2026-01-01', sections: [] },
    ])
  })

  it('ignores headings without a version and everything under them', () => {
    const parsed = parseChangelog(
      ['## Unreleased', '### Added', '* nope', '## 1.0.0 (2026-01-01)', '### Added', '* yes'].join('\n'),
    )

    expect(parsed).toEqual([
      { version: '1.0.0', date: '2026-01-01', sections: [{ title: 'Added', items: [{ text: 'yes' }] }] },
    ])
  })

  it('handles CRLF line endings', () => {
    const parsed = parseChangelog('## 1.0.0 (2026-01-01)\r\n\r\n### Added\r\n\r\n* item\r\n')

    expect(parsed[0].sections[0].items).toEqual([{ text: 'item' }])
  })
})

describe('isVersion', () => {
  it.each([
    ['1.2.3', true],
    ['1.2.3-beta.1', true],
    ['1.2', false],
    ['garbage', false],
    ['', false],
  ])('isVersion(%j) is %s', (value, expected) => {
    expect(isVersion(value)).toBe(expected)
  })
})

describe('compareVersions', () => {
  it.each([
    ['1.10.0', '1.9.0', 1],
    ['1.9.0', '1.10.0', -1],
    ['2.0.0', '1.99.99', 1],
    ['1.2.3', '1.2.3', 0],
    ['1.2.0-beta.1', '1.2.0', 0],
  ])('compares %s with %s', (left, right, expected) => {
    expect(Math.sign(compareVersions(left, right))).toBe(expected)
  })
})

function makeRelease(version: string): ChangelogRelease {
  return { version, date: null, sections: [{ title: 'Added', items: [{ text: version }] }] }
}

describe('releasesSince', () => {
  it('returns releases after lastSeen up to current inclusive, newest first, whatever the input order', () => {
    const releases = ['1.0.0', '1.2.0', '1.1.0', '1.3.0'].map(makeRelease)

    expect(releasesSince(releases, '1.0.0', '1.2.0').map((release) => release.version)).toEqual(['1.2.0', '1.1.0'])
  })

  it('returns nothing when lastSeen equals current', () => {
    expect(releasesSince(['1.0.0', '1.1.0'].map(makeRelease), '1.1.0', '1.1.0')).toEqual([])
  })

  it('does not mutate its input', () => {
    const releases = ['1.0.0', '1.2.0', '1.1.0'].map(makeRelease)

    releasesSince(releases, '0.9.0', '1.2.0')

    expect(releases.map((release) => release.version)).toEqual(['1.0.0', '1.2.0', '1.1.0'])
  })
})
