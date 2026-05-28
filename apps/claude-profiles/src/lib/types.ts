export type Surfaces = {
  gui: boolean
  cli: boolean
}

export type Profile = {
  id: string
  name: string
  slug: string
  color: string
  createdAt: string
  surfaces: Surfaces
  /**
   * RFC 3339 timestamp of the last `launched_gui` or `copied_cli` event,
   * or `null` if this profile has never been used.
   */
  lastUsedAt: string | null
}

export type ActivityKind =
  | 'created'
  | 'renamed'
  | 'color_changed'
  | 'surface_toggled'
  | 'launched_gui'
  | 'copied_cli'
  | 'imported'

export type Activity = {
  kind: ActivityKind
  at: string
  metadata?: Record<string, unknown> | null
}

export type AppError = {
  kind: 'Io' | 'Json' | 'Validation' | 'NotFound'
  message: string
}

export type Surface = 'gui' | 'cli'

export type ProfilePatch = {
  name?: string
  color?: string
}

export type ProfilePaths = {
  dataDir: string
  guiDataDir: string
  cliConfigDir: string
  guiLauncherPath: string
  cliWrapperPath: string
}

export type ExistingInstallInfo = {
  claudeDesktopPath: string | null
  claudeCodePath: string | null
  /**
   * Bytes on disk for each detected install. `null` when the corresponding
   * path is also `null` (nothing detected) OR when the size walk hasn't
   * run yet — the boot-time `detect_existing_claude_install` IPC returns
   * `null` here to keep startup fast; sizes arrive later via
   * `detect_existing_claude_sizes`. Permission-denied subpaths during the
   * walk are silently skipped on the Rust side, so the eventual value is
   * best-effort.
   */
  claudeDesktopSizeBytes: number | null
  claudeCodeSizeBytes: number | null
}

export type ExistingInstallSizes = {
  claudeDesktopSizeBytes: number | null
  claudeCodeSizeBytes: number | null
}

export type ImportExistingInput = {
  name: string
  color: string
  includeGui: boolean
  includeCli: boolean
}

export type MigrationBackupInfo = {
  path: string
  createdAtMs: number
  sizeBytes: number
  eligibleForCleanup: boolean
}

export type Dependencies = {
  claudeAppInstalled: boolean
  claudeCliInstalled: boolean
  localBinOnPath: boolean
}

export type Shell = 'zsh' | 'bash' | 'fish'

export type PathHookOutcome =
  | { outcome: 'alreadyInstalled'; rcPath: string }
  | { outcome: 'installed'; rcPath: string; backupPath: string }

export type ThemeMode = 'light' | 'system' | 'dark'

export type AppState = {
  welcomeShown: boolean
  migrationDismissedAt: string | null
  pathBannerDismissedAt: string | null
  themeMode: ThemeMode
}

export type AppStatePatch = {
  welcomeShown?: boolean
  migrationDismissedAt?: string
  pathBannerDismissedAt?: string
  themeMode?: ThemeMode
  clearMigrationDismissed?: boolean
  clearPathBannerDismissed?: boolean
}

/**
 * What the About dialog renders. Sourced from `Cargo.toml` via Cargo's
 * `env!` macros on the Rust side, so editing the manifest (e.g. setting
 * `repository = "https://github.com/…"`) automatically populates the
 * dialog on next build.
 */
export type AppMetadata = {
  name: string
  version: string
  description: string
  authors: Array<string>
  repository: string | null
  homepage: string | null
  license: string | null
}

export type UsageWindow = {
  utilization: number | null
  resetsAt: string | null
}

export type QuotaUsage = {
  fiveHour: UsageWindow | null
  sevenDay: UsageWindow | null
  sevenDaySonnet: UsageWindow | null
}

export type QuotaError = 'no_credentials' | 'unauthorized' | 'rate_limited' | 'network' | 'unknown'

export type ProfileUsage = {
  quota: QuotaUsage | null
  quotaError: QuotaError | null
  fetchedAt: string
}
