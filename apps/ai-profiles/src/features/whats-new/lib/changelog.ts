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
 * Where `parseChangelog` is in the file.
 */
type ParserState = {
  /**
   * Releases parsed so far, in file order.
   */
  releases: Array<ChangelogRelease>
  /**
   * The release under the latest `## …` heading, or `null` while outside one (preamble, `## Unreleased`).
   */
  release: ChangelogRelease | null
  /**
   * The section under the latest `### …` heading, or `null` while outside one.
   */
  section: ChangelogSection | null
  /**
   * The raw text of the item being read, or `null` while not inside one.
   */
  pendingItem: string | null
}

/**
 * Handles one changelog line, advancing the parser.
 */
type LineHandler = (state: ParserState, line: string) => void

/**
 * Adds the item being read, if any, to the current section.
 */
function flushItem(state: ParserState): void {
  if (state.section !== null && state.pendingItem !== null) {
    state.section.items.push(toItem(state.pendingItem))
  }
  state.pendingItem = null
}

/**
 * A `## …` line: starts a release when the heading carries a semver.
 */
function startRelease(state: ParserState, line: string): void {
  flushItem(state)
  state.section = null
  state.release = createRelease(line)
  if (state.release !== null) {
    state.releases.push(state.release)
  }
}

/**
 * A `### …` line: starts a section of the current release. Ignored outside a release.
 */
function startSection(state: ParserState, line: string): void {
  flushItem(state)
  if (state.release === null) {
    state.section = null
    return
  }
  state.section = { title: line.slice(4).replace(warningPrefixPattern, '').trim(), items: [] }
  state.release.sections.push(state.section)
}

/**
 * A `* …` line: starts an item.
 */
function startItem(state: ParserState, line: string): void {
  flushItem(state)
  state.pendingItem = line.slice(2)
}

/**
 * Any other line: a blank one ends the item being read; a non-blank one continues it (wrapped text).
 */
function continueItem(state: ParserState, line: string): void {
  if (line.trim() === '') {
    flushItem(state)
    return
  }
  if (state.pendingItem !== null) {
    state.pendingItem = `${state.pendingItem} ${line.trim()}`
  }
}

/**
 * The handler for a line, picked by its prefix.
 */
function handlerFor(line: string): LineHandler {
  if (line.startsWith('## ')) {
    return startRelease
  }
  if (line.startsWith('### ')) {
    return startSection
  }
  if (line.startsWith('* ')) {
    return startItem
  }
  return continueItem
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
  const state: ParserState = { releases: [], release: null, section: null, pendingItem: null }

  for (const line of raw.split(/\r?\n/)) {
    handlerFor(line)(state, line)
  }
  flushItem(state)

  return state.releases.map((entry) => ({
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
