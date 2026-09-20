import type { ChangelogRelease } from './changelog'

import { compareVersions, isVersion, releasesSince } from './changelog'

type ResolveWhatsNewInput = {
  /**
   * Version of the running app.
   */
  current: string
  /**
   * Version recorded on the previous launch, or `null` when there is none (fresh install).
   */
  lastSeen: string | null
  /**
   * Every parsed release, in any order.
   */
  releases: Array<ChangelogRelease>
}

export type WhatsNewResolution = {
  /**
   * True when the app was upgraded since the last launch and there are notes to show.
   */
  upgraded: boolean
  /**
   * What the dialog should list: the unseen releases (newest first) after an upgrade, otherwise
   * the current release alone, or nothing when the changelog has no entry for it.
   */
  releases: Array<ChangelogRelease>
}

/**
 * Decides whether this launch follows an upgrade and which release notes to show. Only releases
 * with at least one item count as notes.
 */
export function resolveWhatsNew({ current, lastSeen, releases }: ResolveWhatsNewInput): WhatsNewResolution {
  const notes = releases.filter((release) => release.sections.length > 0)
  const currentNotes = notes.find((release) => release.version === current)
  const currentOnly: WhatsNewResolution = {
    upgraded: false,
    releases: currentNotes === undefined ? [] : [currentNotes],
  }

  if (lastSeen === null || !isVersion(lastSeen) || compareVersions(current, lastSeen) <= 0) {
    return currentOnly
  }

  const unseen = releasesSince(notes, lastSeen, current)
  if (unseen.length === 0) {
    return currentOnly
  }

  return { upgraded: true, releases: unseen }
}
