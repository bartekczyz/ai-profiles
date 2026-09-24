import type { AppId } from './app-registry'
import type {
  AccountStatus,
  AppMetadata,
  AppState,
  AppStatePatch,
  ArchiveCheck,
  ArchivedSession,
  ArchiveReport,
  DeletedAccount,
  Dependencies,
  DirListing,
  ExistingInstallInfo,
  ExistingInstallSizes,
  HostInfo,
  ImportExistingInput,
  LaunchResult,
  LoginStart,
  MigrationBackupInfo,
  NewRemoteSession,
  PairingPreview,
  PathHookOutcome,
  Profile,
  ProfilePatch,
  ProfilePaths,
  ProfileUsage,
  RemoteAccount,
  RemoteArchivedSession,
  RemoteHost,
  RemoteLaunch,
  RemoteMoveProgress,
  RemoteRenameSessionResult,
  RemoteSession,
  RemoteTransferPlan,
  RemoteTransferReport,
  RemoteTransferRequest,
  RestoreCheck,
  RestoreReport,
  SessionSummary,
  Shell,
  Surface,
  Surfaces,
  TransferPlan,
  TransferReport,
  TransferRequest,
  WindowKey,
  WindowScreen,
} from './types'

import { invoke } from '@tauri-apps/api/core'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'

export function listProfiles(): Promise<Array<Profile>> {
  return invoke<Array<Profile>>('list_profiles')
}

export function createProfile(input: {
  app: AppId
  name: string
  color: string
  surfaces: Surfaces
  /**
   * Opt-in: leaving it out gives the profile the plain script launcher.
   */
  distinctDockIcon?: boolean
}): Promise<Profile> {
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

export function openProfileInApp(id: string): Promise<LaunchResult> {
  return invoke<LaunchResult>('open_profile_in_app', { id })
}

export function touchProfileLastUsed(id: string): Promise<Profile> {
  return invoke<Profile>('touch_profile_last_used', { id })
}

export function openInFinder(path: string): Promise<void> {
  return invoke('open_in_finder', { path })
}

export function openDefaultGui(app: AppId, dataDir: string): Promise<void> {
  return invoke('open_default_gui', { app, dataDir })
}

export function profilePaths(id: string): Promise<ProfilePaths> {
  return invoke<ProfilePaths>('profile_paths', { id })
}

export function profileAccount(id: string): Promise<AccountStatus> {
  return invoke<AccountStatus>('profile_account', { id })
}

export function copyToClipboard(text: string): Promise<void> {
  return writeText(text)
}

export function detectExistingInstall(app: AppId): Promise<ExistingInstallInfo> {
  return invoke<ExistingInstallInfo>('detect_existing_install', { app })
}

export function detectExistingSizes(app: AppId): Promise<ExistingInstallSizes> {
  return invoke<ExistingInstallSizes>('detect_existing_sizes', { app })
}

export function importExistingInstall(app: AppId, input: ImportExistingInput): Promise<Profile> {
  return invoke<Profile>('import_existing_install', { app, input })
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

export function openCliLogin(id: string): Promise<void> {
  return invoke('open_cli_login', { id })
}

export function getProfileUsage(profileId: string): Promise<ProfileUsage> {
  return invoke<ProfileUsage>('get_profile_usage', { profileId })
}

export function listSessions(id: string): Promise<Array<SessionSummary>> {
  return invoke<Array<SessionSummary>>('list_sessions', { id })
}

export function planSessionTransfer(request: TransferRequest): Promise<TransferPlan> {
  return invoke<TransferPlan>('plan_session_transfer', { request })
}

/** Claude's merge of memory note `path`, which the move needs decided. */
export function mergeTransferMemory(request: TransferRequest, path: string): Promise<string> {
  return invoke<string>('merge_transfer_memory', { request, path })
}

export function transferSession(request: TransferRequest): Promise<TransferReport> {
  return invoke<TransferReport>('transfer_session', { request })
}

export function checkSessionArchive(input: { profileId: string; sessionId: string }): Promise<ArchiveCheck> {
  return invoke<ArchiveCheck>('check_session_archive', input)
}

export function archiveSession(input: {
  profileId: string
  sessionId: string
  /** Quit the profile's desktop app first when it has to. */
  quitApp: boolean
}): Promise<ArchiveReport> {
  return invoke<ArchiveReport>('archive_session', input)
}

export function listArchivedSessions(id: string): Promise<Array<ArchivedSession>> {
  return invoke<Array<ArchivedSession>>('list_archived_sessions', { id })
}

export function checkSessionRestore(input: {
  profileId: string
  sessionId: string
  archive: string
}): Promise<RestoreCheck> {
  return invoke<RestoreCheck>('check_session_restore', input)
}

/** Delete an archived session for good. Resolves to what it freed, in bytes. */
export function deleteArchivedSession(input: {
  profileId: string
  sessionId: string
  archive: string
}): Promise<number> {
  return invoke<number>('delete_archived_session', input)
}

export function restoreSession(input: {
  profileId: string
  sessionId: string
  archive: string
  /** Quit the profile's desktop app first when it has to. */
  quitApp: boolean
}): Promise<RestoreReport> {
  return invoke<RestoreReport>('restore_session', input)
}

export function remoteListHosts(): Promise<Array<RemoteHost>> {
  return invoke<Array<RemoteHost>>('remote_list_hosts')
}

export function remotePreviewPairing(code: string): Promise<PairingPreview> {
  return invoke<PairingPreview>('remote_preview_pairing', { code })
}

export function remotePairHost(input: { code: string; label?: string }): Promise<RemoteHost> {
  return invoke<RemoteHost>('remote_pair_host', { code: input.code, label: input.label ?? null })
}

export function remoteRenameHost(input: { hostId: string; label: string }): Promise<RemoteHost> {
  return invoke<RemoteHost>('remote_rename_host', input)
}

export function remoteRemoveHost(hostId: string): Promise<void> {
  return invoke('remote_remove_host', { hostId })
}

export function remoteHostInfo(hostId: string): Promise<HostInfo> {
  return invoke<HostInfo>('remote_host_info', { hostId })
}

export function remoteListAccounts(hostId: string): Promise<Array<RemoteAccount>> {
  return invoke<Array<RemoteAccount>>('remote_list_accounts', { hostId })
}

export function remoteListSessions(input: { hostId: string; account: string }): Promise<Array<RemoteSession>> {
  return invoke<Array<RemoteSession>>('remote_list_sessions', input)
}

export function remoteListDirs(input: { hostId: string; path?: string }): Promise<DirListing> {
  return invoke<DirListing>('remote_list_dirs', { hostId: input.hostId, path: input.path ?? null })
}

export function remoteNewSession(input: {
  hostId: string
  account: string
  request: NewRemoteSession
}): Promise<RemoteLaunch> {
  return invoke<RemoteLaunch>('remote_new_session', input)
}

/** Ends a running session on the host. Resolves to whether it was running. */
export function remoteStopSession(input: { hostId: string; account: string; sessionId: string }): Promise<boolean> {
  return invoke<boolean>('remote_stop_session', input)
}

/** Stops a session and resumes it, on whatever `claude` the host has now. */
export function remoteRestartSession(input: {
  hostId: string
  account: string
  sessionId: string
  trustFolder: boolean
}): Promise<RemoteLaunch> {
  return invoke<RemoteLaunch>('remote_restart_session', input)
}

/** What moving a session to account `to` on its host would do. */
export function remoteTransferPlan(input: {
  hostId: string
  account: string
  sessionId: string
  to: string
}): Promise<RemoteTransferPlan> {
  return invoke<RemoteTransferPlan>('remote_transfer_plan', input)
}

/** Move a session to another account on its host. */
export function remoteTransferSession(input: {
  hostId: string
  account: string
  sessionId: string
  request: RemoteTransferRequest
}): Promise<RemoteTransferReport> {
  return invoke<RemoteTransferReport>('remote_transfer_session', input)
}

/** Rename a remote session: its title, the host's registry and Remote Control. */
export function remoteRenameSession(input: {
  hostId: string
  account: string
  sessionId: string
  name: string
}): Promise<RemoteRenameSessionResult> {
  return invoke<RemoteRenameSessionResult>('remote_rename_session', input)
}

/** How far a move on a host has got; `null` before it starts and once it ends. */
export function remoteTransferProgress(input: {
  hostId: string
  progressId: string
}): Promise<RemoteMoveProgress | null> {
  return invoke<RemoteMoveProgress | null>('remote_transfer_progress', input)
}

/** Ask Claude on the host to merge a memory note a move conflicts on. */
export function remoteMergeMemory(input: {
  hostId: string
  account: string
  sessionId: string
  to: string
  path: string
}): Promise<string> {
  return invoke<string>('remote_merge_memory', input)
}

/** Archive a session on its host. Resolves to where its transcript went. */
export function remoteArchiveSession(input: { hostId: string; account: string; sessionId: string }): Promise<string> {
  return invoke<string>('remote_archive_session', input)
}

export function remoteArchivedSessions(input: {
  hostId: string
  account: string
}): Promise<Array<RemoteArchivedSession>> {
  return invoke<Array<RemoteArchivedSession>>('remote_archived_sessions', input)
}

/** Put an archived session back. Resolves to where its transcript went. */
export function remoteRestoreSession(input: {
  hostId: string
  account: string
  sessionId: string
  archive: string
}): Promise<string> {
  return invoke<string>('remote_restore_session', input)
}

/** Delete an archived remote session for good. Resolves to what it freed, in bytes. */
export function remoteDeleteArchive(input: {
  hostId: string
  account: string
  sessionId: string
  archive: string
}): Promise<number> {
  return invoke<number>('remote_delete_archive', input)
}

export function remoteResumeSession(input: {
  hostId: string
  account: string
  sessionId: string
  trustFolder: boolean
}): Promise<RemoteLaunch> {
  return invoke<RemoteLaunch>('remote_resume_session', input)
}

export function remoteCreateAccount(input: { hostId: string; name: string }): Promise<RemoteAccount> {
  return invoke<RemoteAccount>('remote_create_account', input)
}

/** Where a remote session opened: the Claude app it went to, or claude.ai and why. */
export type OpenedInClaude = {
  app: string | null
  web: string | null
  note: string | null
}

/**
 * Open a remote session's Remote Control view in the Claude app on this Mac
 * signed in as `email`, starting it if needed; on claude.ai otherwise.
 */
export function remoteOpenInClaude(input: { email: string | null; bridgeSessionId: string }): Promise<OpenedInClaude> {
  return invoke<OpenedInClaude>('remote_open_in_claude', input)
}

/** Rename a remote profile: its account (folder) on the host. */
export function remoteRenameAccount(input: {
  hostId: string
  account: string
  newName: string
  stopRunning: boolean
}): Promise<RemoteAccount> {
  return invoke<RemoteAccount>('remote_rename_account', input)
}

/** Give a remote profile its color. Resolves to the host, with it saved. */
export function remoteSetProfileColor(input: { hostId: string; account: string; color: string }): Promise<RemoteHost> {
  return invoke<RemoteHost>('remote_set_profile_color', input)
}

/** Sign a remote profile out. Resolves to how many running sessions were stopped. */
export function remoteLogout(input: { hostId: string; account: string; stopRunning: boolean }): Promise<number> {
  return invoke<number>('remote_logout', input)
}

export function remoteDeleteAccount(input: { hostId: string; account: string }): Promise<DeletedAccount> {
  return invoke<DeletedAccount>('remote_delete_account', input)
}

/** Starts a sign-in on the host and opens its page in the browser. */
export function remoteLoginStart(input: { hostId: string; account: string }): Promise<LoginStart> {
  return invoke<LoginStart>('remote_login_start', input)
}

export function remoteLoginSubmit(input: { hostId: string; loginId: string; code: string }): Promise<RemoteAccount> {
  return invoke<RemoteAccount>('remote_login_submit', input)
}

export function remoteLoginCancel(input: { hostId: string; loginId: string }): Promise<void> {
  return invoke('remote_login_cancel', input)
}

export function remoteWindowScreen(input: {
  hostId: string
  account: string
  windowId: string
}): Promise<WindowScreen> {
  return invoke<WindowScreen>('remote_window_screen', input)
}

/** Types into a window the host opened; resolves to what it shows after. */
export function remoteWindowKeys(input: {
  hostId: string
  account: string
  windowId: string
  keys: Array<WindowKey>
}): Promise<WindowScreen> {
  return invoke<WindowScreen>('remote_window_keys', input)
}

/**
 * Opens a Terminal window attached over ssh to a remote tmux window. The
 * attach command is checked in Rust; nothing else from the host is run.
 */
/**
 * Opens Terminal attached to the window over ssh, after checking ssh gets in.
 * Resolves to a hint when Terminal is going to ask something (a password, a
 * new host key); rejects, saying what to set up, when ssh can't get in.
 */
export function remoteOpenInTerminal(input: { hostId: string; attachCommand: string }): Promise<string | null> {
  return invoke<string | null>('remote_open_in_terminal', input)
}
