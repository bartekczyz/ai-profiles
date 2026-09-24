/**
 * Typed query-key factory.
 *
 * Hierarchical: `keys.profiles.detail(id)` is a child of `keys.profiles.all`,
 * so a single invalidation of `keys.profiles.all` invalidates every detail
 * subtree. Adding new feature keys is a matter of nesting another object.
 */
export const queryKeys = {
  profiles: {
    all: ['profiles'] as const,
    detail: (id: string) => ['profiles', id] as const,
    paths: (id: string) => ['profiles', id, 'paths'] as const,
    account: (id: string) => ['profiles', id, 'account'] as const,
  },
  // Per-profile Anthropic usage stats. Deliberately OUTSIDE the
  // `profiles` subtree so a prefix invalidation of `['profiles']`
  // (which fires on reorder/delete/migration) doesn't refetch every
  // visible profile's quota in parallel and trip the rate limiter.
  profileUsage: (id: string) => ['profile-usage', id] as const,
  // A profile's Claude sessions, and the plan for moving one. Outside the
  // `profiles` subtree for the same reason as usage: reading every transcript
  // is not something a reorder should set off.
  sessions: {
    all: ['sessions'] as const,
    list: (id: string) => ['sessions', id] as const,
    archived: (id: string) => ['sessions', id, 'archived'] as const,
    transferPlan: (request: object) => ['sessions', 'transfer-plan', request] as const,
    archiveCheck: (profileId: string, sessionId: string) =>
      ['sessions', 'archive-check', profileId, sessionId] as const,
    restoreCheck: (profileId: string, sessionId: string, archive: string) =>
      ['sessions', 'restore-check', profileId, sessionId, archive] as const,
  },
  // Remote hosts and what they report. Never persisted (the provider only
  // dehydrates usage), and outside `profiles` so a local reorder doesn't
  // refetch a machine across the network.
  remote: {
    all: ['remote'] as const,
    hosts: ['remote', 'hosts'] as const,
    info: (hostId: string) => ['remote', hostId, 'info'] as const,
    accounts: (hostId: string) => ['remote', hostId, 'accounts'] as const,
    sessions: (hostId: string, account: string) => ['remote', hostId, 'accounts', account, 'sessions'] as const,
    archived: (hostId: string, account: string) => ['remote', hostId, 'accounts', account, 'archived'] as const,
    transferPlan: (hostId: string, account: string, sessionId: string, to: string) =>
      ['remote', hostId, 'accounts', account, 'transfer', sessionId, to] as const,
    dirs: (hostId: string, path: string | null) => ['remote', hostId, 'dirs', path] as const,
    move: (hostId: string, progressId: string | null) => ['remote', hostId, 'moves', progressId] as const,
    window: (hostId: string, account: string, windowId: string) =>
      ['remote', hostId, 'accounts', account, 'windows', windowId] as const,
  },
  dependencies: ['dependencies'] as const,
  migration: {
    existing: ['migration', 'existing'] as const,
    sizes: ['migration', 'sizes'] as const,
    backups: ['migration', 'backups'] as const,
  },
  appState: ['app-state'] as const,
  shell: ['shell'] as const,
} as const
