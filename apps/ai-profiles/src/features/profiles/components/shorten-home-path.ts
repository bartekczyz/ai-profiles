/**
 * Rewrites the leading home directory of an absolute path as `~`, so a
 * resolved path reads at a glance in tight UI (menu rows, status lines).
 * Paths outside the home directory come back unchanged.
 *
 * `home` is the home folder to shorten against, for paths from another
 * machine (a remote host's `/home/<user>`); without it, a Mac home
 * (`/Users/<user>`) is recognised.
 */
export function shortenHomePath(absolutePath: string, home?: string): string {
  if (home !== undefined) {
    const trimmed = home.replace(/\/+$/, '')
    if (trimmed.length > 0 && (absolutePath === trimmed || absolutePath.startsWith(`${trimmed}/`))) {
      return `~${absolutePath.slice(trimmed.length)}`
    }
    return absolutePath
  }
  const macHome = absolutePath.match(/^\/Users\/[^/]+/)?.[0]
  if (macHome === undefined) {
    return absolutePath
  }
  return `~${absolutePath.slice(macHome.length)}`
}
