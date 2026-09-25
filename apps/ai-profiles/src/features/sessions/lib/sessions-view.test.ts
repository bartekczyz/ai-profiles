import type { Session } from '@/lib/types'
import type { SessionsListing, ViewFilters } from './sessions-view'

import { describe, expect, it } from 'vitest'

import { makeSession } from '../test/make-session'
import { emptyTabCopy, sessionsView } from './sessions-view'

/**
 * The ids of `sessions`, in order.
 */
function ids(sessions: Array<Session>): Array<string> {
  return sessions.map((session) => session.id)
}

/**
 * Two active CLI sessions, oldest first, a desktop one that needs repair, and
 * an archived one.
 */
const sessions = [
  makeSession({ id: 'old', title: 'Fix the build', lastUsedAt: '2026-09-01T10:00:00Z' }),
  makeSession({ id: 'new', title: 'Refactor the parser', lastUsedAt: '2026-09-03T10:00:00Z' }),
  makeSession({ id: 'desk', kind: 'desktop', title: 'Plan', needsRepair: true, lastUsedAt: '2026-09-02T10:00:00Z' }),
  makeSession({ id: 'gone', title: 'Old parser work', archived: true, needsRepair: true }),
]

/**
 * A listing that landed with `sessions`.
 */
const listed: SessionsListing = { sessions, isError: false, error: null }

/**
 * The Active tab, unnarrowed, newest first.
 */
const activeFilters: ViewFilters = { tab: 'active', kindChoice: 'all', query: '', direction: 'desc' }

describe('sessionsView', () => {
  it('shows the open tab’s sessions, narrowed and ordered, with each tab’s count', () => {
    const view = sessionsView(listed, { ...activeFilters, query: 'the', direction: 'asc' })
    expect(ids(view.visible)).toEqual(['old', 'new'])
    expect(view.tabTotal).toBe(3)
    expect(view.counts).toEqual({ active: 3, archived: 1 })
    expect(view.footerShown).toBe(true)
  })

  it('applies the kind chosen while the tab mixes both kinds', () => {
    const view = sessionsView(listed, { ...activeFilters, kindChoice: 'desktop' })
    expect(view.kindFilterShown).toBe(true)
    expect(view.kind).toBe('desktop')
    expect(ids(view.visible)).toEqual(['desk'])
  })

  it('sets the kind chosen aside while the tab holds one kind', () => {
    const view = sessionsView(listed, { ...activeFilters, tab: 'archived', kindChoice: 'desktop' })
    expect(view.kindFilterShown).toBe(false)
    expect(view.kind).toBe('all')
    expect(ids(view.visible)).toEqual(['gone'])
  })

  it('offers repair for every session that needs it, whichever tab is open', () => {
    expect(sessionsView(listed, { ...activeFilters, tab: 'archived' }).repairSessionIds).toEqual(['desk', 'gone'])
  })

  it('shows no counts, footer or failure until the first listing lands', () => {
    const view = sessionsView({ isError: false, error: null }, activeFilters)
    expect(view.counts).toBeUndefined()
    expect(view.visible).toEqual([])
    expect(view.footerShown).toBe(false)
    expect(view.failure).toBeNull()
    expect(view.refreshFailed).toBe(false)
  })

  it('keeps the footer while the tab holds sessions, even when none match, and drops it on an empty tab', () => {
    expect(sessionsView(listed, { ...activeFilters, tab: 'archived', query: 'nothing' }).footerShown).toBe(true)
    expect(sessionsView({ ...listed, sessions: [] }, activeFilters).footerShown).toBe(false)
  })

  it('describes a listing that never landed', () => {
    const view = sessionsView({ isError: true, error: { kind: 'Io', message: 'boom' } }, activeFilters)
    expect(view.failure).toEqual({ missingTool: false, message: 'boom' })
    expect(view.refreshFailed).toBe(false)
  })

  it('keeps the last list when a refresh fails, noting it instead of describing it', () => {
    const view = sessionsView({ ...listed, isError: true, error: { kind: 'Io', message: 'boom' } }, activeFilters)
    expect(view.failure).toBeNull()
    expect(view.refreshFailed).toBe(true)
    expect(ids(view.visible)).toEqual(['new', 'desk', 'old'])
  })
})

describe('emptyTabCopy', () => {
  it('hints on an empty Active tab where sessions come from, in the words of the app’s CLI', () => {
    expect(emptyTabCopy('active', 'codex').hint).toContain('Codex')
  })

  it('gives an empty Archived tab no hint', () => {
    expect(emptyTabCopy('archived', 'claude').hint).toBeUndefined()
  })
})
