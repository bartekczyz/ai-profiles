import type {
  ExistingInstallInfo,
  ImportExistingInput,
  MigrationBackupInfo,
  Profile,
  ProfilePatch,
  ProfilePaths,
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

export function toggleSurface(input: { id: string; surface: Surface; enabled: boolean }): Promise<Profile> {
  return invoke<Profile>('toggle_surface', input)
}

export function openProfileInApp(id: string): Promise<void> {
  return invoke('open_profile_in_app', { id })
}

export function openInFinder(path: string): Promise<void> {
  return invoke('open_in_finder', { path })
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

export function importExistingInstall(input: ImportExistingInput): Promise<Profile> {
  return invoke<Profile>('import_existing_install', { input })
}

export function listMigrationBackups(): Promise<Array<MigrationBackupInfo>> {
  return invoke<Array<MigrationBackupInfo>>('list_migration_backups')
}

export function deleteMigrationBackup(path: string): Promise<void> {
  return invoke('delete_migration_backup', { path })
}
