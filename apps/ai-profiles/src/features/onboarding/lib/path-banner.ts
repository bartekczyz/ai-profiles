const sevenDaysMs = 7 * 24 * 60 * 60 * 1000
const dismissalWindowMs = 7 * sevenDaysMs

/**
 * What decides whether the PATH setup banner shows.
 */
type PathBannerInput = {
  /**
   * Whether onboarding's welcome dialog has been seen.
   */
  welcomeShown: boolean
  /**
   * Whether `~/.local/bin` is already on the user's PATH.
   */
  localBinOnPath: boolean
  /**
   * Whether any profile has a CLI wrapper that needs PATH.
   */
  anyCliProfile: boolean
  /**
   * RFC 3339 timestamp of the last dismissal, or `null` if never dismissed.
   */
  dismissedAt: string | null
  /**
   * The current time in epoch milliseconds.
   */
  now: number
}

/**
 * Pure: the banner nags only after onboarding, only when a CLI wrapper exists
 * that would need `~/.local/bin` on PATH, and not within 49 days of a dismissal.
 */
export function shouldShowPathBanner({
  welcomeShown,
  localBinOnPath,
  anyCliProfile,
  dismissedAt,
  now,
}: PathBannerInput): boolean {
  const dismissedRecently = dismissedAt !== null && now - new Date(dismissedAt).getTime() < dismissalWindowMs
  return welcomeShown && !localBinOnPath && anyCliProfile && !dismissedRecently
}
