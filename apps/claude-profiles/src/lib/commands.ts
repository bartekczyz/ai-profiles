import type {
  Activity,
  ActivityKind,
  AppMetadata,
  AppState,
  AppStatePatch,
  Dependencies,
  ExistingInstallInfo,
  ExistingInstallSizes,
  ImportExistingInput,
  MigrationBackupInfo,
  PathHookOutcome,
  Profile,
  ProfilePatch,
  ProfilePaths,
  ProfileUsage,
  Shell,
  Surface,
  Surfaces,
} from './types'

import { invoke } from '@tauri-apps/api/core'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'

export function listProfiles(): Promise<Array<Profile>> {
  return invoke<Array<Profile>>('list_profiles')
}

export function createProfile(input: { name: string; color: string; surfaces: Surfaces }): Promise<Profile> {
  return invoke<Profile>('create_profile', input)
}

export function updateProfile(input: { id: string; patch: ProfilePatch }): Promise<Profile> {
  return invoke<Profile>('update_profile', input)
}

export function deleteProfile(input: { id: string; moveToTrash: boolean }): Promise<void> {
  return invoke('delete_profile', input)
}

export function reorderProfiles(ids: Array<string>): Promise<Array<Profile>> {
  return invoke<Array<Profile>>('reorder_profiles', { ids })
}

export function toggleSurface(input: { id: string; surface: Surface; enabled: boolean }): Promise<Profile> {
  return invoke<Profile>('toggle_surface', input)
}

export function openProfileInApp(id: string): Promise<Profile> {
  return invoke<Profile>('open_profile_in_app', { id })
}

export function listActivity(input: { profileId: string; limit: number }): Promise<Array<Activity>> {
  return invoke<Array<Activity>>('list_activity', input)
}

export function recordActivity(input: {
  profileId: string
  kind: ActivityKind
  metadata?: Record<string, unknown> | null
}): Promise<Profile> {
  return invoke<Profile>('record_activity', input)
}

export function openInFinder(path: string): Promise<void> {
  return invoke('open_in_finder', { path })
}

export function openClaudeGui(dataDir: string): Promise<void> {
  return invoke('open_claude_gui', { dataDir })
}

export function profilePaths(id: string): Promise<ProfilePaths> {
  return invoke<ProfilePaths>('profile_paths', { id })
}

export function copyToClipboard(text: string): Promise<void> {
  return writeText(text)
}

export function detectExistingClaudeInstall(): Promise<ExistingInstallInfo> {
  return invoke<ExistingInstallInfo>('detect_existing_claude_install')
}

export function detectExistingClaudeSizes(): Promise<ExistingInstallSizes> {
  return invoke<ExistingInstallSizes>('detect_existing_claude_sizes')
}

export function importExistingInstall(input: ImportExistingInput): Promise<Profile> {
  return invoke<Profile>('import_existing_install', { input })
}

export function listMigrationBackups(): Promise<Array<MigrationBackupInfo>> {
  return invoke<Array<MigrationBackupInfo>>('list_migration_backups')
}

export function deleteMigrationBackup(path: string): Promise<void> {
  return invoke('delete_migration_backup', { path })
}

export function checkDependencies(): Promise<Dependencies> {
  return invoke<Dependencies>('check_dependencies')
}

export function detectShell(): Promise<Shell> {
  return invoke<Shell>('detect_shell')
}

export function installPathHook(shell: Shell): Promise<PathHookOutcome> {
  return invoke<PathHookOutcome>('install_path_hook', { shell })
}

export function loadAppState(): Promise<AppState> {
  return invoke<AppState>('load_app_state')
}

export function updateAppState(patch: AppStatePatch): Promise<AppState> {
  return invoke<AppState>('update_app_state', { patch })
}

export function getAppMetadata(): Promise<AppMetadata> {
  return invoke<AppMetadata>('get_app_metadata')
}

export function openExternalUrl(url: string): Promise<void> {
  return invoke('open_external_url', { url })
}

export function getProfileUsage(profileId: string): Promise<ProfileUsage> {
  return invoke<ProfileUsage>('get_profile_usage', { profileId })
}
