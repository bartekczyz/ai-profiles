import type { Session } from '@/lib/types'

/**
 * A CLI session with every optional field empty, overridden per case.
 */
export function makeSession(overrides: Partial<Session> = {}): Session {
  return {
    id: 's1',
    kind: 'cli',
    title: null,
    cwd: null,
    lastPrompt: null,
    lastUsedAt: '2026-09-01T10:00:00Z',
    archived: false,
    state: 'idle',
    needsRepair: false,
    unmovableReason: null,
    ...overrides,
  }
}
