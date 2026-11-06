import type { Session, SessionCost } from '@/lib/types'

import { describe, expect, it } from 'vitest'

import { makeSession } from '../test/make-session'
import { countByTab, filterSessions, hasBothKinds, sortSessions } from './session-filters'

/**
 * The ids of `sessions`, in order.
 */
function ids(sessions: Array<Session>): Array<string> {
  return sessions.map((session) => session.id)
}

/**
 * A cost of `usd`, covering as much of the use as `coverage` says.
 */
function makeCost(usd: number, coverage: SessionCost['coverage'] = 'complete'): SessionCost {
  return { totalUsd: usd, byModel: [{ model: 'Opus 5.5', usd }], unpriced: [], coverage }
}

describe('filterSessions', () => {
  const sessions = [
    makeSession({ id: 'titled', title: 'Refactor the Parser' }),
    makeSession({ id: 'folder', cwd: '/Users/ada/Developer/billing' }),
    makeSession({ id: 'prompt', lastPrompt: 'why does the build fail?' }),
    makeSession({ id: 'archived', title: 'Old parser work', archived: true }),
    makeSession({ id: 'desktop', kind: 'desktop', title: 'Desktop chat' }),
  ]

  it('splits sessions by tab', () => {
    expect(ids(filterSessions(sessions, { tab: 'active', kind: 'all', query: '' }))).toEqual([
      'titled',
      'folder',
      'prompt',
      'desktop',
    ])
    expect(ids(filterSessions(sessions, { tab: 'archived', kind: 'all', query: '' }))).toEqual(['archived'])
  })

  it.each([
    ['title', 'parser', ['titled']],
    ['cwd', 'BILLING', ['folder']],
    ['last prompt', 'build fail', ['prompt']],
  ])('matches the %s case-insensitively', (_field, query, expected) => {
    expect(ids(filterSessions(sessions, { tab: 'active', kind: 'all', query }))).toEqual(expected)
  })

  it('ignores surrounding whitespace in the query', () => {
    expect(ids(filterSessions(sessions, { tab: 'active', kind: 'all', query: '  parser  ' }))).toEqual(['titled'])
  })

  it('treats a blank query as no search', () => {
    expect(filterSessions(sessions, { tab: 'active', kind: 'all', query: '   ' })).toHaveLength(4)
  })

  it('keeps only the chosen kind', () => {
    expect(ids(filterSessions(sessions, { tab: 'active', kind: 'desktop', query: '' }))).toEqual(['desktop'])
    expect(ids(filterSessions(sessions, { tab: 'active', kind: 'cli', query: '' }))).toEqual([
      'titled',
      'folder',
      'prompt',
    ])
  })

  it('applies tab, kind and query together', () => {
    expect(ids(filterSessions(sessions, { tab: 'archived', kind: 'cli', query: 'parser' }))).toEqual(['archived'])
    expect(filterSessions(sessions, { tab: 'archived', kind: 'desktop', query: 'parser' })).toEqual([])
  })
})

describe('sortSessions', () => {
  const sessions = [
    makeSession({ id: 'middle', lastUsedAt: '2026-09-02T10:00:00Z' }),
    makeSession({ id: 'newest', lastUsedAt: '2026-09-03T10:00:00Z' }),
    makeSession({ id: 'oldest', lastUsedAt: '2026-09-01T10:00:00Z' }),
  ]

  it('puts the most recently used first when descending', () => {
    expect(ids(sortSessions(sessions, { key: 'lastUsed', direction: 'desc' }))).toEqual(['newest', 'middle', 'oldest'])
  })

  it('puts the least recently used first when ascending', () => {
    expect(ids(sortSessions(sessions, { key: 'lastUsed', direction: 'asc' }))).toEqual(['oldest', 'middle', 'newest'])
  })

  it('leaves the input untouched', () => {
    sortSessions(sessions, { key: 'lastUsed', direction: 'asc' })
    expect(ids(sessions)).toEqual(['middle', 'newest', 'oldest'])
  })

  it('compares instants, not strings, across time zone offsets', () => {
    const offset = [
      makeSession({ id: 'earlier', lastUsedAt: '2026-09-02T11:00:00+02:00' }),
      makeSession({ id: 'later', lastUsedAt: '2026-09-02T10:00:00Z' }),
    ]
    expect(ids(sortSessions(offset, { key: 'lastUsed', direction: 'desc' }))).toEqual(['later', 'earlier'])
  })
})

describe('sortSessions by cost', () => {
  const sessions = [
    makeSession({ id: 'cheap', lastUsedAt: '2026-09-01T10:00:00Z' }),
    makeSession({ id: 'unpriced', lastUsedAt: '2026-09-02T10:00:00Z' }),
    makeSession({ id: 'pricey', lastUsedAt: '2026-09-03T10:00:00Z' }),
    makeSession({ id: 'none', lastUsedAt: '2026-09-04T10:00:00Z' }),
    makeSession({ id: 'cheap-newer', lastUsedAt: '2026-09-05T10:00:00Z' }),
  ]
  const costs = {
    cheap: makeCost(2),
    'cheap-newer': makeCost(2),
    pricey: makeCost(40, 'partial'),
    unpriced: makeCost(0, 'unpriced'),
  }

  it('puts the priciest first, ties most recently used first, and rows without a figure last', () => {
    expect(ids(sortSessions(sessions, { key: 'cost', direction: 'desc' }, costs))).toEqual([
      'pricey',
      'cheap-newer',
      'cheap',
      'none',
      'unpriced',
    ])
  })

  it('keeps rows without a figure last when the cheapest come first', () => {
    expect(ids(sortSessions(sessions, { key: 'cost', direction: 'asc' }, costs))).toEqual([
      'cheap-newer',
      'cheap',
      'pricey',
      'none',
      'unpriced',
    ])
  })
})

describe('hasBothKinds', () => {
  it('is true when desktop and CLI sessions are mixed', () => {
    expect(hasBothKinds([makeSession({ kind: 'cli' }), makeSession({ kind: 'desktop' })])).toBe(true)
  })

  it('is false for a single kind', () => {
    expect(hasBothKinds([makeSession({ kind: 'cli' }), makeSession({ kind: 'cli' })])).toBe(false)
    expect(hasBothKinds([makeSession({ kind: 'desktop' })])).toBe(false)
  })

  it('is false for no sessions', () => {
    expect(hasBothKinds([])).toBe(false)
  })
})

describe('countByTab', () => {
  it('counts active and archived sessions', () => {
    expect(
      countByTab([makeSession(), makeSession({ archived: true }), makeSession(), makeSession({ archived: true })]),
    ).toEqual({ active: 2, archived: 2 })
  })

  it('counts zero for an empty list', () => {
    expect(countByTab([])).toEqual({ active: 0, archived: 0 })
  })
})
