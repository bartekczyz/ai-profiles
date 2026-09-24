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
  kind: 'Io' | 'Json' | 'Validation' | 'NotFound' | 'Remote'
  message: string
  /**
   * For `Remote`: the server's code (`unauthorized`, `not_found`, …) or the
   * client's own (`offline`, `cert_mismatch`).
   */
  code?: string
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
  /**
   * Delete the source's copy afterwards instead, once everything moved is
   * checked to be identical. Can't be undone. Not with `archiveSource`.
   */
  deleteSource: boolean
  replaceNewer?: boolean
  /** Quit the apps the plan lists in `appsToQuit` first. */
  quitApps?: boolean
  /** What to keep of each memory note both profiles changed, by path. */
  memory?: Record<string, MemoryDecision>
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
  /** What deleting the source's copy afterwards frees, in bytes. */
  sourceBytes: number
  /** What archiving the source's copy takes, in bytes, before compression. */
  archiveBytes: number
  /** The project's memory, file by file; its conflicts need deciding. */
  memory: Array<TransferMemoryFile>
}

/** How far a move has got, as it runs: its steps in order, and the one it's on. */
export type TransferProgress = {
  sessionId: string
  steps: Array<string>
  /** An index into `steps`. */
  current: number
}

export type TransferReport = {
  destinationTranscript: string
  backupDir: string | null
  desktopRecord: string | null
  archivedTo: string | null
  /** What deleting the source's copy freed, in bytes, when asked for. */
  freedBytes: number | null
  /** Why the source's copy was kept although deleting it was asked for. */
  deleteError: string | null
  /** What merging the project's memory did, one line per file. */
  memory: Array<string>
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
  /** What the archive takes on disk, in bytes. */
  sizeBytes: number
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

/** The account a profile is signed in under, from what its CLI keeps on disk. */
export type ProfileAccount = {
  email: string | null
  /** The person's name, as the app recorded it. */
  name: string | null
  organization: string | null
  /** The subscription, e.g. "Max" or "Pro". */
  plan: string | null
}

/**
 * Whether a profile is signed in, and as whom. `unknown` is signed in, or may
 * be, as an account nothing on disk names: a Claude desktop app signed in as
 * someone its `.claude.json` doesn't mention.
 */
export type AccountStatus =
  | { status: 'signedIn'; account: ProfileAccount }
  | { status: 'signedOut' }
  | { status: 'unknown' }

/** A Linux machine running ai-profiles-server that this Mac is paired with. */
export type RemoteHost = {
  id: string
  /** What the sidebar calls it. */
  label: string
  hostname: string
  addresses: Array<string>
  /** SHA-256 of its certificate, lowercase hex. */
  fingerprint: string
  clientId: string
  pairedAt: string
  lastGoodAddress: string | null
  /** How each of its accounts looks as a profile here, by account name. */
  profiles: Record<string, RemoteProfileLook>
}

/** How a remote profile looks in the sidebar. The account itself is the host's. */
export type RemoteProfileLook = {
  color: string
}

/** What a pairing code says, shown before pairing. */
export type PairingPreview = {
  addresses: Array<string>
  /** `AB:CD:…` */
  fingerprint: string
}

export type HostInfo = {
  hostname: string
  home: string
  serverVersion: string
  apiVersion: number
  tmux: { version: string; session: string } | null
  claude: { path: string; version: string | null } | null
  accountsBase: string
  includesDefault: boolean
}

/** A Claude account on a remote host: one CLAUDE_CONFIG_DIR. */
export type RemoteAccount = {
  name: string
  isDefault: boolean
  configDir: string
  account: ProfileAccount | null
  signedIn: boolean
  /** When the sign-in runs out (RFC 3339), if Claude recorded it. */
  signedInUntil: string | null
  sessions: number
  runningSessions: number
}

export type TmuxWindow = {
  session: string
  /** `@7` */
  windowId: string
  paneId: string
}

export type RemoteSession = {
  id: string
  cwd: string | null
  title: string | null
  /** Someone chose the name (Remote Control uses it). */
  named: boolean
  lastPrompt: string | null
  /** RFC 3339. */
  updatedAt: string
  sizeBytes: number
  running: boolean
  window: TmuxWindow | null
  /** Remote Control is connected. */
  remoteControl: boolean
  /** Its id on claude.ai while Remote Control is connected (`session_…`). */
  bridgeSessionId: string | null
  /**
   * Started, but Claude hasn't registered it yet: its window is asking
   * something first, such as whether to trust the folder.
   */
  waiting?: boolean
  /** Started moments ago with Remote Control, which hasn't connected yet. */
  remoteControlConnecting?: boolean
  /** Nothing has been said in it yet: once stopped, it leaves the list. */
  empty?: boolean
  /** The Claude Code version it runs, while it runs. */
  claudeVersion?: string | null
  /**
   * A newer Claude Code is installed than the one it runs ("Update installed ·
   * Restart to update"): a restart takes it on.
   */
  updatePending?: boolean
}

/** The subfolders of a folder on a remote host, for picking where to work. */
export type DirListing = {
  path: string
  /** The folder above, while it's still inside what the host allows. */
  parent: string | null
  home: string
  entries: Array<{ name: string; path: string }>
  truncated: boolean
}

/** Something a started window is waiting on. */
export type Attention = {
  /** `trustPrompt`, or `waiting` for anything else. */
  kind: string
  text: string
}

/** Where a started (or already running) remote session is. */
export type RemoteLaunch = {
  window: TmuxWindow
  alreadyRunning: boolean
  sessionId: string | null
  remoteControlName: string | null
  attention: Attention | null
  /** To run on the host: `tmux attach -t … \; select-window -t …`. */
  attachCommand: string
}

export type NewRemoteSession = {
  cwd: string
  name?: string
  trustFolder: boolean
}

/** A running Claude a start, resume or move on a host would run beside. */
export type RemoteRunningMatch = {
  account: string
  pid: number
  sessionId: string | null
  cwd: string | null
  /** The very session. */
  exact: boolean
  window: TmuxWindow | null
}

/** What a move does with each item of the session, as claudemulti plans it. */
export type RemoteTransferItem = {
  /** Relative to the destination account. */
  path: string
  action: 'copy' | 'same' | 'replace' | 'remove'
}

/** One project memory file of a move, local or on a host. */
export type TransferMemoryFile = {
  /** Relative to the project's memory folder. */
  path: string
  action: 'add' | 'same' | 'index' | 'merge' | 'conflict'
  newer: 'source' | 'destination'
  /** Both texts, for a conflict. */
  sourceText: string | null
  destinationText: string | null
}

export type RemoteTransferMemoryFile = TransferMemoryFile

/** What moving a session to another account on its host would do. */
export type RemoteTransferPlan = {
  sessionId: string
  source: string
  destination: string
  title: string | null
  cwd: string | null
  items: Array<RemoteTransferItem>
  /** The destination's copy is newer: the move would roll it back. */
  destinationNewer: boolean
  memory: Array<RemoteTransferMemoryFile>
  /** Claude already running with the session or in its folder. */
  running: Array<RemoteRunningMatch>
  /** What deleting the source's copy afterwards frees, in bytes. */
  sourceBytes: number
  /** What archiving the source's copy takes, in bytes, before compression. */
  archiveBytes: number
}

/** How a memory conflict is settled. */
export type MemoryDecision = { take: 'source' } | { take: 'destination' } | { take: 'merged'; text: string }

export type RemoteTransferRequest = {
  to: string
  /** Exit the session first if it's running, so the move takes all of it. */
  stopFirst: boolean
  replaceNewer: boolean
  confirmRunning: boolean
  memory: Record<string, MemoryDecision>
  archiveSource: boolean
  /**
   * Delete the source's copy afterwards instead, once the moved copy is
   * checked to be identical. Can't be undone. Not with `archiveSource`.
   */
  deleteSource: boolean
  resume: boolean
  trustFolder: boolean
  /** Chosen here, to follow the move while it runs. */
  progressId?: string
}

/** What a remote session was renamed to. */
export type RemoteRenameSessionResult = {
  /** Its name now: Claude adds a suffix when another live session holds the one asked for. */
  name: string
  /** It was running, so Remote Control has the name now; otherwise it takes it on resume. */
  live: boolean
}

/** How far a move on a host has got. */
export type RemoteMoveProgress = {
  /** What it does, in order. */
  steps: Array<string>
  /** The step it's on now, an index into `steps`. */
  current: number
}

/** What a move on a host did. */
export type RemoteTransferReport = {
  changed: boolean
  backupDir: string | null
  /** One line per memory file, as claudemulti prints them. */
  memory: Array<string>
  archivedTo: string | null
  /** What deleting the source's copy freed, in bytes, when asked for. */
  freedBytes: number | null
  /** Why the source's copy was kept although deleting it was asked for. */
  deleteError: string | null
  launch: RemoteLaunch | null
  resumeError: string | null
}

/** An archived session on a host. */
export type RemoteArchivedSession = {
  id: string
  /** `<stamp>-archived`. */
  archive: string
  /** `%Y%m%d-%H%M%S`, the host's local time. */
  stamp: string
  title: string | null
  cwd: string | null
  /** What the archive takes on disk, in bytes. */
  sizeBytes: number
}

/** Where a deleted remote account's folder went (its host's `.trash`). */
export type DeletedAccount = {
  trashedTo: string
}

/** A sign-in under way on a remote host. */
export type LoginStart = {
  loginId: string
  /** The sign-in page, already opened in the browser. */
  url: string
  /** RFC 3339. */
  expiresAt: string
}

/** What a tmux window the host opened shows now. */
export type WindowScreen = {
  /** The visible screen, one line per row. */
  text: string
  width: number
  height: number
  /** Set once Claude in the window has registered its session. */
  sessionId: string | null
  /** Remote Control is connected. */
  remoteControl: boolean
}

/** One thing to type into a window: a tmux key name (`Enter`, `Down`, `C-c`, …) or plain text. */
export type WindowKey = { key: string } | { text: string }
