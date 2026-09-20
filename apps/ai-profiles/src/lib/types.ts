import type { AppId } from './app-registry'

export type { AppId } from './app-registry'

export type Surfaces = {
  gui: boolean
  cli: boolean
}

export type Profile = {
  id: string
  app: AppId
  name: string
  slug: string
  color: string
  createdAt: string
  surfaces: Surfaces
  /**
   * Whether the desktop launcher is a wrapper app with a Dock identity of its
   * own (icon, label, pinnable tile) rather than a script that opens the stock
   * app. Off for profiles saved before the setting existed.
   */
  distinctDockIcon: boolean
  /**
   * RFC 3339 timestamp of the last `launched_gui` or `copied_cli` event,
   * or `null` if this profile has never been used.
   */
  lastUsedAt: string | null
}

export type DefaultEntry = {
  id: string
  app: AppId
  name: string
  surfaces: Surfaces
}

export type SidebarEntry = { kind: 'managed'; profile: Profile } | { kind: 'default'; entry: DefaultEntry }

export type AppError = {
  kind: 'Io' | 'Json' | 'Validation' | 'NotFound'
  message: string
}

export type Surface = 'gui' | 'cli'

export type ProfilePatch = {
  name?: string
  color?: string
  /**
   * Switching this rebuilds the launcher in the other shape.
   */
  distinctDockIcon?: boolean
}

/**
 * Why a profile's own Dock-icon launcher was skipped for one launch.
 */
export type WrapperBypass = {
  /**
   * A sentence on what went wrong with the launcher.
   */
  reason: string
}

/**
 * What opening a profile's desktop app came to.
 */
export type LaunchResult = {
  /**
   * The profile, with its last-used time stamped.
   */
  profile: Profile
  /**
   * Set when the profile asks for a launcher of its own that was left out of
   * this launch, with why. The setting itself is untouched.
   */
  wrapperBypass: WrapperBypass | null
}

export type ProfilePaths = {
  dataDir: string
  guiDataDir: string
  cliConfigDir: string
  guiLauncherPath: string | null
  cliWrapperPath: string | null
}

export type ExistingInstallInfo = {
  guiPath: string | null
  cliPath: string | null
  /**
   * Bytes on disk for each detected install. `null` when the corresponding
   * path is also `null` (nothing detected) OR when the size walk hasn't
   * run yet — the boot-time `detect_existing_install` IPC returns `null`
   * here to keep startup fast; sizes arrive later via `detect_existing_sizes`.
   * Permission-denied subpaths during the walk are silently skipped on the
   * Rust side, so the eventual value is best-effort.
   */
  guiSizeBytes: number | null
  cliSizeBytes: number | null
}

export type ExistingInstallSizes = {
  guiSizeBytes: number | null
  cliSizeBytes: number | null
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

export type AppDependency = {
  guiInstalled: boolean
  cliInstalled: boolean
}

export type Dependencies = {
  apps: Record<AppId, AppDependency>
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
  selectedEntryId: string | null
  /**
   * When the user first confirmed they understand what giving a profile its own
   * Dock icon involves. `null` until then, which is when the explanation is shown.
   */
  dockIconAcknowledgedAt: string | null
}

export type AppStatePatch = {
  welcomeShown?: boolean
  migrationDismissedAt?: string
  pathBannerDismissedAt?: string
  themeMode?: ThemeMode
  clearMigrationDismissed?: boolean
  clearPathBannerDismissed?: boolean
  selectedEntryId?: string | null
  clearSelectedEntryId?: boolean
  /**
   * Records the acknowledgement. It cannot be taken back.
   */
  dockIconAcknowledgedAt?: string
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
  primary: UsageWindow | null
  secondary: UsageWindow | null
  secondaryExtra: UsageWindow | null
}

export type QuotaError =
  | 'no_credentials'
  | 'unauthorized'
  | 'forbidden'
  | 'needs_login'
  | 'rate_limited'
  | 'network'
  | 'unknown'

export type ProfileUsage = {
  quota: QuotaUsage | null
  quotaError: QuotaError | null
  fetchedAt: string
}
