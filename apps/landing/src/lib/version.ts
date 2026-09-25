import desktopPackage from '../../../ai-profiles/package.json'

// The version of the desktop app, sourced at build time from
// apps/ai-profiles/package.json (release-please updates that
// file on every release). Prefix with `v` to match the
// GitHub Releases tag convention.
const desktopVersionRaw: string = desktopPackage.version
export const desktopVersion: string = `v${desktopVersionRaw}`

// Computed default .dmg filename. The actual filename comes from
// the GitHub release at runtime; this is the fallback shown
// before the JS swap completes.
export const defaultDmgFilename: string = `ai-profiles-${desktopVersionRaw}.dmg`
