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
  /** The custom name when the user gave one, else the app's display name. */
  name: string
  /** The name the user gave this entry, or `null` for the stock label. */
  customName: string | null
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
  /** Names the user gave the stock-install entries. Absent key → stock label. */
  defaultProfileNames: Partial<Record<AppId, string>>
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
  /**
   * Renames one app's stock-install entry. An empty name restores the stock label.
   */
  defaultProfileName?: { app: AppId; name: string }
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
  windowDurationMins?: number | null
  /**
   * Server-supplied display name for a window the client can't label on its
   * own — the model a scoped weekly quota applies to, e.g. `Fable`. Absent
   * for windows whose label is fixed copy.
   */
  label?: string | null
}

export type RateLimitResetCredits = {
  availableCount: number
  credits: Array<{ title: string | null; status: string; expiresAt: number | null }> | null
}

/**
 * Pay-as-you-go credit spend. Amounts stay in the currency's minor units
 * (pence, cents) exactly as the backend reports them, so no rounding
 * happens before the formatter sees them.
 */
export type Spend = {
  /**
   * Amount consumed this period, in minor units.
   */
  usedMinor: number
  /**
   * ISO-4217 code the amounts are denominated in, e.g. `GBP`.
   */
  currency: string
  /**
   * Decimal places the minor units carry — 2 for `GBP`, 0 for `JPY`.
   */
  exponent: number
  /**
   * Spend cap in minor units. Null for an uncapped account.
   */
  limitMinor: number | null
  /**
   * Server-computed share of the cap consumed, on a 0..=100 scale.
   */
  percent: number | null
}

export type QuotaUsage = {
  primary: UsageWindow | null
  secondary: UsageWindow | null
  /**
   * Weekly sub-quotas scoped to a single model, each carrying its own
   * `label`. Empty for apps that have none.
   */
  scopedWeekly: Array<UsageWindow>
  rateLimitResetCredits?: RateLimitResetCredits
  spend?: Spend
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

/**
 * One Claude session a profile keeps, as the Sessions panel lists it.
 */
export type SessionSummary = {
  id: string
  /** The folder the session last worked in. */
  cwd: string | null
  /** Desktop-app name, else the `/rename` name, else Claude's generated one. */
  title: string | null
  lastPrompt: string | null
  /** RFC 3339. */
  updatedAt: string
  sizeBytes: number
  /** A `claude` process has it open right now. */
  running: boolean
  /** That process is the desktop app's, which holds it open until it quits. */
  openInDesktop: boolean
  /** The profile's desktop app lists it. */
  inDesktop: boolean
  /** Why it can't be moved, if it can't. */
  unmovableReason: string | null
  /**
   * The profile's desktop app lists it, but its transcript is in the Default
   * folder, where the app wrote it before it had a config dir of its own.
   */
  leftInDefault: boolean
}

export type TransferRequest = {
  sourceId: string
  sessionId: string
  destinationId: string
  addToDesktop: boolean
  archiveSource: boolean
  replaceNewer?: boolean
  /** Quit the apps the plan lists in `appsToQuit` first. */
  quitApps?: boolean
}

/**
 * A profile's desktop app that has to quit before a move or archive: it holds
 * the session open, or keeps the session list being changed.
 */
export type AppToQuit = {
  profileId: string
  label: string
}

export type TransferItemAction = 'copy' | 'same' | 'replace'

export type TransferDesktopAction = 'skip' | 'add' | 'alreadyListed' | 'unavailable'

export type TransferPlan = {
  sessionId: string
  title: string | null
  cwd: string | null
  sourceLabel: string
  destinationLabel: string
  items: Array<{ path: string; action: TransferItemAction }>
  /** The destination's copy is newer: moving would roll it back. */
  destinationNewer: boolean
  desktop: TransferDesktopAction
  desktopReason: string | null
  /** Why the move can't happen right now, which only the user can clear. */
  blockers: Array<string>
  /** Apps that have to quit first. ai-profiles quits them when asked. */
  appsToQuit: Array<AppToQuit>
  notes: Array<string>
}

export type TransferReport = {
  destinationTranscript: string
  backupDir: string | null
  desktopRecord: string | null
  archivedTo: string | null
  memoryCopied: Array<string>
  memoryConflicts: Array<string>
}

export type ArchiveReport = {
  /** Where the transcript (and desktop record) went. */
  archivedTo: string
}

export type ArchiveCheck = {
  /** A reason only the user can clear (a terminal has it open). */
  blocker: string | null
  /** The profile's desktop app, if it has to quit first. */
  appToQuit: AppToQuit | null
}

/** A session a profile has archived, as the Archived list shows it. */
export type ArchivedSession = {
  id: string
  /** The archive folder, `<time>-archived`: which archive of the session. */
  archive: string
  /** RFC 3339. */
  archivedAt: string | null
  title: string | null
  cwd: string | null
  /** Restoring lists it in the desktop app again. */
  inDesktop: boolean
}

export type RestoreCheck = {
  /** A reason only the user can clear (a live copy is already back). */
  blocker: string | null
  /** The profile's desktop app, if it has to quit first. */
  appToQuit: AppToQuit | null
}

export type RestoreReport = {
  transcript: string
}
