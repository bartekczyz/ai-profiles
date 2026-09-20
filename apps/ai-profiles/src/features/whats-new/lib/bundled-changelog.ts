import changelog from '../../../../CHANGELOG.md?raw'
import { parseChangelog } from './changelog'

/**
 * Release notes parsed from `apps/ai-profiles/CHANGELOG.md`, inlined at build time. release-please
 * updates that file in the release PR, so a tagged build always contains its own entry. Parsed once
 * at module load.
 */
export const bundledReleases = parseChangelog(changelog)
