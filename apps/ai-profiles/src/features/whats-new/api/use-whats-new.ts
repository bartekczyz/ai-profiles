import type { ChangelogRelease } from '../lib/changelog'

import { useEffect, useState } from 'react'

import { useAppMetadata } from '@/features/about/api/use-app-metadata'

import { bundledReleases } from '../lib/bundled-changelog'
import { resolveWhatsNew } from '../lib/resolve-whats-new'
import { readLastSeenVersion, writeLastSeenVersion } from './last-seen-version'

type WhatsNew = {
  /**
   * Version of the running app.
   */
  version: string
  /**
   * True when this launch follows an upgrade that has release notes.
   */
  upgraded: boolean
  /**
   * Releases the dialog should list (see `resolveWhatsNew`).
   */
  releases: Array<ChangelogRelease>
}

/**
 * Decides once per mount whether the app was just upgraded and which release notes to show, then
 * records the running version so the next launch compares against it. The decision is a snapshot
 * (lazy state) so recording the version doesn't flip `upgraded` back mid-session. Suspends while
 * the app metadata loads, so mount it under a `Suspense` boundary.
 */
export function useWhatsNew(): WhatsNew {
  const { version } = useAppMetadata()
  const [resolution] = useState(() =>
    resolveWhatsNew({ current: version, lastSeen: readLastSeenVersion(), releases: bundledReleases }),
  )

  useEffect(() => {
    writeLastSeenVersion(version)
  }, [version])

  return { version, ...resolution }
}
