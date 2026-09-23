import type { Session } from '@/lib/types'

import { describe, expect, it } from 'vitest'

import { closeInTerminalReason, rowActions } from './session-actions'

/**
 * A session with every optional field empty, overridden per case.
 */
function makeSession(overrides: Partial<Session> = {}): Session {
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

describe('rowActions', () => {
  it('offers an active Claude session archiving', () => {
    expect(rowActions(makeSession(), 'claude')).toEqual([{ action: 'archive' }])
  })

  it('offers an archived Claude session restoring', () => {
    expect(rowActions(makeSession({ archived: true }), 'claude')).toEqual([{ action: 'restore' }])
  })

  it('holds archiving back while a terminal has the session open', () => {
    expect(rowActions(makeSession({ state: 'openInTerminal' }), 'claude')).toEqual([
      { action: 'archive', disabledReason: closeInTerminalReason },
    ])
  })

  it('lets a session whose transcript is gone, or that a desktop app has open, be archived', () => {
    expect(rowActions(makeSession({ kind: 'desktop', state: 'transcriptMissing' }), 'claude')).toEqual([
      { action: 'archive' },
    ])
    expect(rowActions(makeSession({ kind: 'desktop', state: 'openInDesktop' }), 'claude')).toEqual([
      { action: 'archive' },
    ])
  })

  it('offers Codex sessions nothing yet', () => {
    expect(rowActions(makeSession(), 'codex')).toEqual([])
    expect(rowActions(makeSession({ archived: true }), 'codex')).toEqual([])
  })
})
