export type ChangelogItem = {
  /**
   * Entry text with the scope prefix and PR/commit links removed.
   */
  text: string
  /**
   * Conventional-commit scope (e.g. `ui`). Absent when the entry has none.
   */
  scope?: string
}

export type ChangelogSection = {
  /**
   * Section heading as written in the changelog (`Added`, `Fixed`, `BREAKING CHANGES`, …).
   */
  title: string
  /**
   * Entries under the heading. Never empty — empty sections are dropped while parsing.
   */
  items: Array<ChangelogItem>
}

export type ChangelogRelease = {
  /**
   * Semver of the release, e.g. `1.1.0`.
   */
  version: string
  /**
   * Release date as `YYYY-MM-DD`, or `null` when the heading has none.
   */
  date: string | null
  /**
   * Non-empty sections in file order. May be empty for a release with no user-visible entries.
   */
  sections: Array<ChangelogSection>
}

const releaseHeadingPattern = /^## .*?(\d+\.\d+\.\d+)/
const releaseDatePattern = /\((\d{4}-\d{2}-\d{2})\)\s*$/
const scopePattern = /^\*\*([^*]+):\*\*\s+/
// `([#37](https://…))` / `([a427b39](https://…))` — release-please's trailing PR and commit references.
const referenceGroupPattern = /\s*\(\[[^\]]*\]\([^)]*\)\)/g
const markdownLinkPattern = /\[([^\]]*)\]\([^)]*\)/g
const warningPrefixPattern = /^⚠️?\s*/
const versionPattern = /^(\d+)\.(\d+)\.(\d+)/

function createRelease(heading: string): ChangelogRelease | null {
  const versionMatch = releaseHeadingPattern.exec(heading)
  if (versionMatch === null) {
    return null
  }

  return { version: versionMatch[1], date: releaseDatePattern.exec(heading)?.[1] ?? null, sections: [] }
}

function toItem(body: string): ChangelogItem {
  const scopeMatch = scopePattern.exec(body)
  const withoutScope = scopeMatch === null ? body : body.slice(scopeMatch[0].length)
  const text = withoutScope
    .replace(referenceGroupPattern, '')
    .replace(markdownLinkPattern, '$1')
    .replace(/\s+/g, ' ')
    .trim()

  return scopeMatch === null ? { text } : { text, scope: scopeMatch[1] }
}

/**
 * Parses release-please's `CHANGELOG.md` into structured releases.
 *
 * A `## …` heading containing a semver starts a release; `### …` starts a section; `* …` starts an
 * item. Non-blank lines directly after an item are joined onto it (wrapped text); a blank line ends
 * the item. Headings without a semver (e.g. `## Unreleased`) and everything under them are ignored,
 * as is the `# Changelog` preamble.
 */
export function parseChangelog(raw: string): Array<ChangelogRelease> {
  const releases: Array<ChangelogRelease> = []
  let release: ChangelogRelease | null = null
  let section: ChangelogSection | null = null
  let pendingItem: string | null = null

  function flushItem() {
    if (section !== null && pendingItem !== null) {
      section.items.push(toItem(pendingItem))
    }
    pendingItem = null
  }

  for (const line of raw.split(/\r?\n/)) {
    if (line.startsWith('## ')) {
      flushItem()
      section = null
      release = createRelease(line)
      if (release !== null) {
        releases.push(release)
      }
    } else if (line.startsWith('### ')) {
      flushItem()
      section = release === null ? null : { title: line.slice(4).replace(warningPrefixPattern, '').trim(), items: [] }
      if (release !== null && section !== null) {
        release.sections.push(section)
      }
    } else if (line.startsWith('* ')) {
      flushItem()
      pendingItem = line.slice(2)
    } else if (line.trim() === '') {
      flushItem()
    } else if (pendingItem !== null) {
      pendingItem = `${pendingItem} ${line.trim()}`
    }
  }
  flushItem()

  return releases.map((entry) => ({
    ...entry,
    sections: entry.sections.filter((entrySection) => entrySection.items.length > 0),
  }))
}

/**
 * True when `value` starts with `major.minor.patch`.
 */
export function isVersion(value: string): boolean {
  return versionPattern.test(value)
}

function toParts(value: string): [number, number, number] {
  const match = versionPattern.exec(value)
  if (match === null) {
    return [0, 0, 0]
  }

  return [Number(match[1]), Number(match[2]), Number(match[3])]
}

/**
 * Numeric major/minor/patch comparison: negative when `a < b`, positive when `a > b`, `0` when
 * equal. Any `-prerelease` suffix is ignored; unparseable input counts as `0.0.0`.
 */
export function compareVersions(a: string, b: string): number {
  const left = toParts(a)
  const right = toParts(b)

  for (let index = 0; index < left.length; index += 1) {
    const difference = left[index] - right[index]
    if (difference !== 0) {
      return difference
    }
  }

  return 0
}

/**
 * Releases with `lastSeen < version <= current`, newest first. Sorts by version rather than
 * trusting file order, and never mutates `releases`.
 */
export function releasesSince(
  releases: Array<ChangelogRelease>,
  lastSeen: string,
  current: string,
): Array<ChangelogRelease> {
  return releases
    .filter(
      (release) => compareVersions(release.version, lastSeen) > 0 && compareVersions(release.version, current) <= 0,
    )
    .sort((left, right) => compareVersions(right.version, left.version))
}
