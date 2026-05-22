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
