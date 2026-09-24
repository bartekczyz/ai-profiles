import { describe, expect, it } from 'vitest'

import { makeSession } from '../test/make-session'
import { moveAvailability, rowActions } from './session-actions'

describe('rowActions', () => {
  it('offers an active session archiving', () => {
    expect(rowActions(makeSession(), 'claude')).toEqual([{ action: 'archive' }])
  })

  it('offers an archived session restoring', () => {
    expect(rowActions(makeSession({ archived: true }), 'claude')).toEqual([{ action: 'restore' }])
  })

  it('holds archiving back while a terminal has the session open', () => {
    expect(rowActions(makeSession({ state: 'openInTerminal' }), 'claude')).toEqual([
      { action: 'archive', disabledReason: 'Close it in the terminal first' },
    ])
  })

  it('says Codex has an open Codex session, whichever of its clients holds it', () => {
    expect(rowActions(makeSession({ state: 'openInTerminal' }), 'codex')).toEqual([
      { action: 'archive', disabledReason: 'Codex has it open — close it first' },
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
})

describe('moveAvailability', () => {
  it('offers moving an active session when there is another profile to move it to', () => {
    expect(moveAvailability(makeSession(), 1)).toEqual({})
  })

  it('holds moving back with the reason the session can’t move', () => {
    expect(moveAvailability(makeSession({ unmovableReason: 'Transcript deleted' }), 2)).toEqual({
      disabledReason: 'Transcript deleted',
    })
  })

  it('offers no move for an archived session, or with nowhere to go', () => {
    expect(moveAvailability(makeSession({ archived: true }), 1)).toBeNull()
    expect(moveAvailability(makeSession(), 0)).toBeNull()
  })
})
