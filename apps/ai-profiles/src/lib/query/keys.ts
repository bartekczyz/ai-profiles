import type { SessionAction } from '@/lib/types'

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
  },
  // Per-profile Anthropic usage stats. Deliberately OUTSIDE the
  // `profiles` subtree so a prefix invalidation of `['profiles']`
  // (which fires on reorder/delete/migration) doesn't refetch every
  // visible profile's quota in parallel and trip the rate limiter.
  profileUsage: (id: string) => ['profile-usage', id] as const,
  dependencies: ['dependencies'] as const,
  migration: {
    existing: ['migration', 'existing'] as const,
    sizes: ['migration', 'sizes'] as const,
    backups: ['migration', 'backups'] as const,
  },
  // Outside the `profiles` subtree: moving a session changes two profiles'
  // lists at once, so every mutation invalidates the whole `sessions` prefix.
  sessions: {
    all: ['sessions'] as const,
    list: (profileId: string) => ['sessions', profileId] as const,
  },
  // Outside the `sessions` subtree: the lists refetching after an action must
  // not refetch the check of the action that was just done.
  sessionActionCheck: (profileId: string, sessionId: string, action: SessionAction) =>
    ['session-action-check', profileId, sessionId, action] as const,
  // Outside the `sessions` subtree for the same reason: the plan of a move
  // that was just done must not refetch and flash what it would do now.
  sessionMovePlan: (profileId: string, sessionId: string, destinationId: string) =>
    ['session-move-plan', profileId, sessionId, destinationId] as const,
  // Outside the `sessions` subtree for the same reason: a repair that was
  // just done must not refetch its check.
  sessionRepairCheck: (profileId: string) => ['session-repair-check', profileId] as const,
  appState: ['app-state'] as const,
  shell: ['shell'] as const,
} as const
