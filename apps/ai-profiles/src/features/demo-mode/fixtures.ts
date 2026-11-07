import type { AppId } from '@/lib/app-registry'
import type {
  AccountStatus,
  ActionCheck,
  ApiCost,
  AppMetadata,
  AppState,
  AppStatePatch,
  Dependencies,
  ExistingInstallInfo,
  ExistingInstallSizes,
  ImportExistingInput,
  LaunchResult,
  MigrationBackupInfo,
  ModelCost,
  MovePlan,
  MoveReport,
  PathHookOutcome,
  Period,
  Profile,
  ProfilePatch,
  ProfilePaths,
  ProfileUsage,
  RepairReport,
  Session,
  SessionCost,
  SessionCosts,
  SessionList,
  Shell,
  Surface,
  Surfaces,
  ThemeMode,
  UsageWindow,
} from '@/lib/types'

import { format } from 'date-fns'

import { appSpecs, wrapperCommand } from '@/lib/app-registry'
import { computeOptimisticAppState } from '@/lib/app-state/use-app-state'

import packageJson from '../../../package.json'

const minuteMs = 60_000
const hourMs = 60 * minuteMs
const dayMs = 24 * hourMs

const home = '/Users/you'
const workId = '6b2f0c1e-4d7a-4b8e-9f21-3a5c7d9e1b04'
const freelanceId = 'c41a9e72-0b3d-4f6a-8e15-7d2b9c4a6f38'
const researchId = '9e7d3b15-62a8-4c0f-b4d9-1f8e5a7c2d60'

/**
 * A profile as the demo keeps it: its times are ages, so they read the same
 * however long after the demo started they are looked at.
 */
type DemoProfile = Omit<Profile, 'createdAt' | 'lastUsedAt'> & {
  /**
   * How long before now it was created.
   */
  createdAgo: number
  /**
   * How long before now it was last used, or `null` if never.
   */
  lastUsedAgo: number | null
}

/**
 * A session as the demo keeps it, with its age and what it cost.
 */
type DemoSession = Omit<Session, 'lastUsedAt'> & {
  /**
   * How long before now it was last used.
   */
  lastUsedAgo: number
  /**
   * What it would cost at API prices, in USD.
   */
  usd: number
}

/**
 * What the write commands change.
 */
export type DemoState = {
  /**
   * The managed profiles, in the user's order.
   */
  profiles: Array<DemoProfile>
  /**
   * What the app keeps between launches: the selection, names given to the
   * default entries, dismissals.
   */
  appState: AppState
  /**
   * Each profile's (or `default:<app>`'s) sessions, by its id.
   */
  sessions: Record<string, Array<DemoSession>>
}

/**
 * What the demo takes from the real app, so showing it changes nothing the
 * app keeps.
 */
export type DemoSeed = {
  /**
   * The theme mode the app is in: the demo's app state reports it, so the
   * app doesn't switch, and store, another.
   */
  themeMode: ThemeMode
}

/**
 * One limit's use: percent used, and how long until it resets.
 */
type DemoWindow = [percent: number, resetsIn: number]

/**
 * What doesn't change about a sidebar entry while the demo runs.
 */
type DemoEntry = {
  /**
   * Who it is signed in as.
   */
  account: AccountStatus
  /**
   * The 5-hour limit.
   */
  primary: DemoWindow
  /**
   * The weekly limit.
   */
  secondary: DemoWindow
  /**
   * What its use over the last 30 days would cost at API prices, in USD.
   */
  monthlyUsd: number
  /**
   * The models it used, with each one's share of the cost.
   */
  models: Array<[model: string, share: number]>
}

function signedIn(email: string, organization: string | null, plan: string): AccountStatus {
  return { status: 'signedIn', account: { email, name: null, organization, plan } }
}

const claudeModels: DemoEntry['models'] = [
  ['Opus 5.5', 0.78],
  ['Sonnet 4.5', 0.19],
  ['Haiku 4.5', 0.03],
]
const codexModels: DemoEntry['models'] = [
  ['GPT-5.3 Codex', 0.84],
  ['GPT-5.6 Terra', 0.16],
]

const entries: Record<string, DemoEntry> = {
  'default:claude': {
    account: signedIn('you@example.net', null, 'Max'),
    primary: [34, 3 * hourMs + 12 * minuteMs],
    secondary: [41, 4 * dayMs + 6 * hourMs],
    monthlyUsd: 318.6,
    models: claudeModels,
  },
  [workId]: {
    account: signedIn('you@example.com', 'Example Inc.', 'Team'),
    // Half a minute over, so the label still reads 2h 44m moments later.
    primary: [12, 2 * hourMs + 44 * minuteMs + 30_000],
    secondary: [88, 2 * dayMs + 5 * hourMs],
    monthlyUsd: 2053.4,
    models: claudeModels,
  },
  [freelanceId]: {
    account: signedIn('you@example.org', null, 'Pro'),
    primary: [5, 4 * hourMs + 20 * minuteMs],
    secondary: [23, 5 * dayMs + 2 * hourMs],
    monthlyUsd: 142.75,
    models: claudeModels,
  },
  'default:codex': {
    account: signedIn('you@example.net', null, 'Plus'),
    primary: [18, hourMs + 50 * minuteMs],
    secondary: [52, 3 * dayMs + 9 * hourMs],
    monthlyUsd: 96.3,
    models: codexModels,
  },
  [researchId]: {
    account: signedIn('you@example.com', 'Example Inc.', 'Pro'),
    primary: [63, 3 * hourMs + 5 * minuteMs],
    secondary: [71, dayMs + 14 * hourMs],
    monthlyUsd: 486.2,
    models: codexModels,
  },
}

type SessionSeed = [title: string, kind: Session['kind'], repo: string, age: number, usd: number, archived?: boolean]

const sessionSeeds: Record<string, Array<SessionSeed>> = {
  'default:claude': [
    ['Garden irrigation timer', 'cli', 'garden-timer', 26 * hourMs, 58.4],
    ['Tax spreadsheet cleanup script', 'desktop', 'scripts', 3 * dayMs, 21.15],
    ['Rust CLI for photo backups', 'cli', 'photo-sync', 6 * dayMs, 112.8],
  ],
  [workId]: [
    ['Onboarding checklist redesign', 'desktop', 'web', 8 * minuteMs, 186.4],
    ['Fix flaky checkout e2e test', 'cli', 'web', 34 * minuteMs, 21.37],
    ['Migrate billing webhooks to queues', 'desktop', 'api', 2 * hourMs + 5 * minuteMs, 512.3],
    ['Design tokens audit', 'desktop', 'design-system', 5 * hourMs + 10 * minuteMs, 241.85],
    ['Rate limiting for the public API', 'cli', 'api', 27 * hourMs, 441.38],
    ['Dark mode for the marketing site', 'desktop', 'marketing-site', 2 * dayMs + 3 * hourMs, 297.2],
    ['Upgrade the web app to React 19', 'cli', 'web', 4 * dayMs + 2 * hourMs, 352.45],
    ['Spike: search with pgvector', 'desktop', 'api', 12 * dayMs, 63.9, true],
  ],
  [freelanceId]: [
    ['Retry failed inventory syncs', 'cli', 'inventory-sync', 2 * dayMs, 48.3],
    ['Quarterly report export', 'cli', 'reports', 5 * dayMs, 31.6],
  ],
  'default:codex': [
    ['Refactor the budget tracker', 'cli', 'budget', 30 * hourMs, 18.9],
    ['README for photo-sync', 'desktop', 'photo-sync', 4 * dayMs, 6.4],
  ],
  [researchId]: [
    ['Benchmark embedding models', 'cli', 'research', 3 * hourMs, 142.7],
    ['Summarise churn interviews', 'desktop', 'research', 28 * hourMs, 64.2],
    ['Prototype recommendation ranking', 'cli', 'ml', 3 * dayMs + 4 * hourMs, 203.5],
  ],
}

function demoSessions(owner: string): Array<DemoSession> {
  return (sessionSeeds[owner] ?? []).map(([title, kind, repo, age, usd, archived = false], index) => ({
    id: `${owner}-session-${index + 1}`,
    kind,
    title,
    cwd: `${home}/code/${repo}`,
    lastPrompt: null,
    lastUsedAgo: age,
    archived,
    state: 'idle',
    needsRepair: false,
    unmovableReason: null,
    usd,
  }))
}

const profileSeeds: Array<Omit<DemoProfile, 'slug' | 'distinctDockIcon'>> = [
  {
    id: workId,
    app: 'claude',
    name: 'Work',
    color: '#6b8db5',
    surfaces: { gui: true, cli: true },
    createdAgo: 140 * dayMs,
    lastUsedAgo: 12 * minuteMs,
  },
  {
    id: freelanceId,
    app: 'claude',
    name: 'Freelance',
    color: '#7c9a6e',
    surfaces: { gui: false, cli: true },
    createdAgo: 45 * dayMs,
    lastUsedAgo: 2 * dayMs,
  },
  {
    id: researchId,
    app: 'codex',
    name: 'Research',
    color: '#b97ab8',
    surfaces: { gui: true, cli: true },
    createdAgo: 80 * dayMs,
    lastUsedAgo: 3 * hourMs,
  },
]

/**
 * A fresh demo, as the screenshots show it: Claude's Work profile selected.
 */
export function createDemoState(seed: DemoSeed): DemoState {
  const owners = ['default:claude', workId, freelanceId, 'default:codex', researchId]
  return {
    profiles: profileSeeds.map((profile) => ({
      ...profile,
      slug: slugify(profile.name),
      distinctDockIcon: profile.surfaces.gui,
    })),
    appState: {
      welcomeShown: true,
      migrationDismissedAt: null,
      pathBannerDismissedAt: null,
      themeMode: seed.themeMode,
      selectedEntryId: workId,
      dockIconAcknowledgedAt: new Date(Date.now() - 140 * dayMs).toISOString(),
      defaultProfileNames: { claude: 'Personal', codex: 'Personal' },
      dismissedRepairSessions: {},
      defaultProfileColors: {},
    },
    sessions: Object.fromEntries(owners.map((owner) => [owner, demoSessions(owner)])),
  }
}

function ago(age: number): string {
  return new Date(Date.now() - age).toISOString()
}

function toProfile({ createdAgo, lastUsedAgo, ...profile }: DemoProfile): Profile {
  return { ...profile, createdAt: ago(createdAgo), lastUsedAt: lastUsedAgo === null ? null : ago(lastUsedAgo) }
}

function findProfile(state: DemoState, id: string): DemoProfile {
  const profile = state.profiles.find((candidate) => candidate.id === id)
  if (!profile) {
    throw { kind: 'NotFound', message: `profile ${id} not found` }
  }
  return profile
}

function defaultApp(id: string): AppId | null {
  if (id === 'default:claude') {
    return 'claude'
  }
  return id === 'default:codex' ? 'codex' : null
}

/**
 * What the sidebar calls the profile or default entry `id`.
 */
function entryName(state: DemoState, id: string): string {
  const app = defaultApp(id)
  if (app) {
    return state.appState.defaultProfileNames[app] ?? appSpecs[app].displayName
  }
  return findProfile(state, id).name
}

function slugify(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
}

function addProfile(state: DemoState, app: AppId, name: string, color: string, surfaces: Surfaces): Profile {
  const profile: DemoProfile = {
    id: crypto.randomUUID(),
    app,
    name,
    slug: slugify(name),
    color,
    surfaces,
    distinctDockIcon: false,
    createdAgo: 0,
    lastUsedAgo: null,
  }
  state.profiles.push(profile)
  state.sessions[profile.id] = []
  return toProfile(profile)
}

function touch(state: DemoState, id: string): Profile {
  const profile = findProfile(state, id)
  profile.lastUsedAgo = 0
  return toProfile(profile)
}

function paths(state: DemoState, id: string): ProfilePaths {
  const app = defaultApp(id)
  if (app) {
    const dir = `${home}/.${app}`
    return {
      dataDir: dir,
      guiDataDir: `${home}/Library/Application Support/${appSpecs[app].displayName}`,
      cliConfigDir: dir,
      guiLauncherPath: `/Applications/${appSpecs[app].displayName}.app`,
      cliWrapperPath: null,
    }
  }
  const profile = findProfile(state, id)
  const dataDir = `${home}/Library/Application Support/ai-profiles/profiles/${id}`
  return {
    dataDir,
    guiDataDir: `${dataDir}/gui-data`,
    cliConfigDir: `${dataDir}/cli-config`,
    guiLauncherPath: `/Applications/${appSpecs[profile.app].displayName} (${profile.name}).app`,
    cliWrapperPath: `${home}/.local/bin/${wrapperCommand(profile.app, profile.slug)}`,
  }
}

function usageWindow([percent, resetsIn]: DemoWindow, minutes: number): UsageWindow {
  return { utilization: percent, resetsAt: new Date(Date.now() + resetsIn).toISOString(), windowDurationMins: minutes }
}

function usage(id: string): ProfileUsage {
  const fetchedAt = new Date().toISOString()
  const entry = entries[id]
  if (!entry) {
    return { quota: null, quotaError: 'no_credentials', fetchedAt }
  }
  return {
    quota: {
      primary: usageWindow(entry.primary, 300),
      secondary: usageWindow(entry.secondary, 10080),
      scopedWeekly: [],
    },
    quotaError: null,
    fetchedAt,
  }
}

function cents(usd: number): number {
  return Math.round(usd * 100) / 100
}

function splitByModel(usd: number, models: DemoEntry['models']): Array<ModelCost> {
  return models.map(([model, share]) => ({ model, usd: cents(usd * share) }))
}

/**
 * Pure: a day's share of an average day's spend, the same whichever period
 * it is read in. Weekends are quieter.
 */
function dayWeight(day: Date): number {
  const dayNumber = Math.floor(day.getTime() / dayMs)
  const noise = Math.abs(Math.sin(dayNumber * 12.9898) * 43758.5453) % 1
  const weekend = day.getDay() === 0 || day.getDay() === 6
  return (0.55 + 0.9 * noise) * (weekend ? 0.35 : 1)
}

function startOfDay(time: number): Date {
  const day = new Date(time)
  day.setHours(0, 0, 0, 0)
  return day
}

/**
 * The days from `from` to `to`, both included, oldest first.
 */
function daysBetween(from: Date, to: Date): Array<Date> {
  const days: Array<Date> = []
  for (let day = startOfDay(from.getTime()); day <= to; day = new Date(day.getTime() + dayMs)) {
    days.push(day)
  }
  return days
}

const periodDays: Record<Exclude<Period, 'yesterday'>, number> = {
  today: 1,
  last7Days: 7,
  last30Days: 30,
  last90Days: 90,
  last6Months: 182,
  year: 365,
}

function periodRange(period: Period): [from: Date, to: Date] {
  const today = startOfDay(Date.now())
  if (period === 'yesterday') {
    const yesterday = new Date(today.getTime() - dayMs)
    return [yesterday, yesterday]
  }
  return [new Date(today.getTime() - (periodDays[period] - 1) * dayMs), today]
}

function pricesAsOf(): string {
  return format(Date.now() - 9 * dayMs, 'yyyy-MM-dd')
}

/**
 * `totalUsd` split across the folders `sessions` worked in, as their own
 * costs are, most expensive first.
 */
function projects(sessions: Array<DemoSession>, totalUsd: number): ApiCost['projects'] {
  const sessionsUsd = sessions.reduce((sum, session) => sum + session.usd, 0)
  const byPath = new Map<string | null, number>()
  for (const session of sessions) {
    byPath.set(session.cwd, (byPath.get(session.cwd) ?? 0) + session.usd)
  }
  return [...byPath]
    .map(([path, usd]) => ({ path, usd: cents((totalUsd * usd) / sessionsUsd) }))
    .sort((left, right) => right.usd - left.usd)
}

/**
 * What profile `id`'s use from `from` to `to` would cost: each day an average
 * day's spend by its weight, scaled so the last 30 days add up to the
 * profile's `monthlyUsd`.
 */
function apiCost(state: DemoState, id: string, from: Date, to: Date): ApiCost {
  const entry = entries[id]
  const [lastMonthFrom, today] = periodRange('last30Days')
  const monthWeight = daysBetween(lastMonthFrom, today).reduce((sum, day) => sum + dayWeight(day), 0)
  const perWeight = (entry?.monthlyUsd ?? 0) / monthWeight
  const days = daysBetween(from, to).map((day) => {
    const totalUsd = cents(dayWeight(day) * perWeight)
    return { date: format(day, 'yyyy-MM-dd'), totalUsd, byModel: splitByModel(totalUsd, entry?.models ?? []) }
  })
  const totalUsd = cents(days.reduce((sum, day) => sum + day.totalUsd, 0))
  return {
    totalUsd,
    projectedUsd: null,
    coveredDays: days.length,
    hasUse: totalUsd > 0,
    days,
    byKind: {
      input: cents(totalUsd * 0.04),
      output: cents(totalUsd * 0.21),
      cacheWrite5m: cents(totalUsd * 0.11),
      cacheWrite1h: cents(totalUsd * 0.05),
      cacheRead: cents(totalUsd * 0.59),
      webSearch: 0,
    },
    byModel: splitByModel(totalUsd, entry?.models ?? []),
    projects: projects(state.sessions[id] ?? [], totalUsd),
    unpriced: [],
    coverage: 'complete',
    pricesAsOf: pricesAsOf(),
  }
}

function toSession({ lastUsedAgo, usd: _usd, ...session }: DemoSession): Session {
  return { ...session, lastUsedAt: ago(lastUsedAgo) }
}

function findSession(state: DemoState, owner: string, id: string): DemoSession {
  const session = state.sessions[owner]?.find((candidate) => candidate.id === id)
  if (!session) {
    throw { kind: 'NotFound', message: `session ${id} not found` }
  }
  return session
}

const nothingInTheWay: ActionCheck = { blocker: null, appToQuit: null }

/**
 * Answers one command from `state`, with the arguments the frontend sent it.
 * Typed `never` here so each handler can name the arguments it takes.
 */
type DemoHandler = (state: DemoState, args: never) => unknown

/**
 * An answer for every app command in `@/lib/commands`, by its name. Reads
 * come from the state and writes change it, so clicking around in demo mode
 * behaves without reaching the backend. Things outside the app (Finder, the
 * desktop apps, the browser) are left alone.
 */
export const demoHandlers: Record<string, DemoHandler> = {
  list_profiles: (state): Array<Profile> => state.profiles.map(toProfile),
  create_profile: (state, input: { app: AppId; name: string; color: string; surfaces: Surfaces }): Profile =>
    addProfile(state, input.app, input.name, input.color, input.surfaces),
  update_profile: (state, { id, patch }: { id: string; patch: ProfilePatch }): Profile => {
    const profile = findProfile(state, id)
    Object.assign(profile, patch, patch.name === undefined ? {} : { slug: slugify(patch.name) })
    return toProfile(profile)
  },
  delete_profile: (state, { id }: { id: string }) => {
    state.profiles = state.profiles.filter((profile) => profile.id !== id)
    delete state.sessions[id]
  },
  reorder_profiles: (state, { ids }: { ids: Array<string> }): Array<Profile> => {
    state.profiles.sort((left, right) => ids.indexOf(left.id) - ids.indexOf(right.id))
    return state.profiles.map(toProfile)
  },
  toggle_surface: (state, { id, surface, enabled }: { id: string; surface: Surface; enabled: boolean }): Profile => {
    const profile = findProfile(state, id)
    profile.surfaces = { ...profile.surfaces, [surface]: enabled }
    return toProfile(profile)
  },
  open_profile_in_app: (state, { id }: { id: string }): LaunchResult => ({
    profile: touch(state, id),
    wrapperBypass: null,
  }),
  touch_profile_last_used: (state, { id }: { id: string }): Profile => touch(state, id),
  open_in_finder: () => undefined,
  open_default_gui: () => undefined,
  profile_paths: (state, { id }: { id: string }): ProfilePaths => paths(state, id),
  profile_account: (_state, { id }: { id: string }): AccountStatus => entries[id]?.account ?? { status: 'signedOut' },
  custom_icon: (): ArrayBuffer => new ArrayBuffer(0),
  generated_icon_preview: (): ArrayBuffer => new ArrayBuffer(0),
  check_custom_icon: () => undefined,
  set_custom_icon: () => undefined,
  clear_custom_icon: () => undefined,
  detect_existing_install: (_state, { app }: { app: AppId }): ExistingInstallInfo => ({
    guiPath: `/Applications/${appSpecs[app].displayName}.app`,
    cliPath: `${home}/.${app}`,
    guiSizeBytes: null,
    cliSizeBytes: null,
  }),
  detect_existing_sizes: (): ExistingInstallSizes => ({ guiSizeBytes: 612_000_000, cliSizeBytes: 48_000_000 }),
  import_existing_install: (state, { app, input }: { app: AppId; input: ImportExistingInput }): Profile =>
    addProfile(state, app, input.name, input.color, { gui: input.includeGui, cli: input.includeCli }),
  list_migration_backups: (): Array<MigrationBackupInfo> => [],
  delete_migration_backup: () => undefined,
  check_dependencies: (): Dependencies => ({
    apps: { claude: { guiInstalled: true, cliInstalled: true }, codex: { guiInstalled: true, cliInstalled: true } },
    localBinOnPath: true,
  }),
  detect_shell: (): Shell => 'zsh',
  install_path_hook: (): PathHookOutcome => ({ outcome: 'alreadyInstalled', rcPath: `${home}/.zshrc` }),
  load_app_state: (state): AppState => state.appState,
  update_app_state: (state, { patch }: { patch: AppStatePatch }): AppState => {
    state.appState = computeOptimisticAppState(state.appState, patch)
    return state.appState
  },
  get_app_metadata: (): AppMetadata => ({
    name: 'ai-profiles',
    version: packageJson.version,
    description:
      'Run multiple Claude accounts on one Mac, each with its own isolated Desktop launcher and CLI wrapper.',
    authors: ['Bartek Czyż <bartek@czyz.it>'],
    repository: 'https://github.com/bartekczyz/ai-profiles',
    homepage: 'https://czyz.it?utm_source=ai-profiles',
    license: 'MIT',
  }),
  open_external_url: () => undefined,
  open_cli_login: () => undefined,
  get_profile_usage: (_state, { profileId }: { profileId: string }): ProfileUsage => usage(profileId),
  get_profile_api_cost: (state, { profileId, period }: { profileId: string; period: Period }): ApiCost =>
    apiCost(state, profileId, ...periodRange(period)),
  get_profile_api_cost_between: (state, args: { profileId: string; from: string; to: string }): ApiCost =>
    apiCost(state, args.profileId, new Date(args.from), new Date(Math.min(Date.parse(args.to), Date.now()))),
  list_sessions: (state, { profileId }: { profileId: string }): SessionList => ({
    sessions: [...(state.sessions[profileId] ?? [])]
      .sort((left, right) => left.lastUsedAgo - right.lastUsedAgo)
      .map(toSession),
    repairCount: 0,
  }),
  get_session_costs: (state, { profileId }: { profileId: string }): SessionCosts => {
    const models = entries[profileId]?.models ?? []
    const sessions = (state.sessions[profileId] ?? []).map((session): [string, SessionCost] => [
      session.id,
      { totalUsd: session.usd, byModel: splitByModel(session.usd, models), unpriced: [], coverage: 'complete' },
    ])
    return { sessions: Object.fromEntries(sessions), pricesAsOf: pricesAsOf() }
  },
  check_session_action: (): ActionCheck => nothingInTheWay,
  archive_session: (state, { profileId, sessionId }: { profileId: string; sessionId: string }) => {
    findSession(state, profileId, sessionId).archived = true
  },
  restore_session: (state, { profileId, sessionId }: { profileId: string; sessionId: string }) => {
    findSession(state, profileId, sessionId).archived = false
  },
  plan_session_move: (state, args: { profileId: string; destinationId: string }): MovePlan => ({
    summary: `Moves 3 files from ${entryName(state, args.profileId)} to ${entryName(state, args.destinationId)}`,
    items: [
      { path: 'projects/session.jsonl', action: 'copy' },
      { path: 'todos/session.json', action: 'copy' },
      { path: 'shell-snapshots/session.sh', action: 'copy' },
    ],
    destinationNewer: false,
    desktop: 'add',
    blockers: [],
    appsToQuit: [],
    notes: [],
  }),
  move_session: (state, args: { profileId: string; sessionId: string; destinationId: string }): MoveReport => {
    const session = findSession(state, args.profileId, args.sessionId)
    state.sessions[args.destinationId] = [...(state.sessions[args.destinationId] ?? []), { ...session }]
    session.archived = true
    return { memoryConflicts: [] }
  },
  check_session_repair: (): ActionCheck => nothingInTheWay,
  repair_sessions: (): RepairReport => ({ repaired: 0, skipped: [], memoryConflicts: [], warnings: [] }),
}
