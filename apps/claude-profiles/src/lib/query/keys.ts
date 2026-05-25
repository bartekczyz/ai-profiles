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
    activity: (id: string) => ['profiles', id, 'activity'] as const,
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
