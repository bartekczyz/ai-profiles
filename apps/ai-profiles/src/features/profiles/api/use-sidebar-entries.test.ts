import type { AppId } from '@/lib/app-registry'
import type { ExistingInstallInfo, SidebarEntry } from '@/lib/types'

import { describe, expect, it } from 'vitest'

import { entryId, groupEntriesByApp, makeDefaultEntries, shortcutEntries } from './use-sidebar-entries'

function existing(overrides: Partial<ExistingInstallInfo> = {}): ExistingInstallInfo {
  return { guiPath: null, cliPath: null, guiSizeBytes: null, cliSizeBytes: null, ...overrides }
}

function byApp(claude: ExistingInstallInfo, codex: ExistingInstallInfo): Record<AppId, ExistingInstallInfo> {
  return { claude, codex }
}

describe('makeDefaultEntries', () => {
  it('returns no entries when neither app is detected', () => {
    expect(makeDefaultEntries(byApp(existing(), existing()))).toEqual([])
  })

  it('emits one entry per detected app, claude before codex', () => {
    const entries = makeDefaultEntries(
      byApp(
        existing({ cliPath: '/Users/me/.claude' }),
        existing({ guiPath: '/Applications/Codex.app', cliPath: '/Users/me/.codex' }),
      ),
    )
    expect(entries.map((entry) => entry.id)).toEqual(['default:claude', 'default:codex'])
    expect(entries[0].surfaces).toEqual({ gui: false, cli: true })
    expect(entries[1].surfaces).toEqual({ gui: true, cli: true })
  })

  it('emits only codex when claude is absent', () => {
    const entries = makeDefaultEntries(byApp(existing(), existing({ guiPath: '/Applications/Codex.app' })))
    expect(entries.map((entry) => entry.id)).toEqual(['default:codex'])
  })

  it('uses the app displayName for the entry name', () => {
    const entries = makeDefaultEntries(byApp(existing({ cliPath: '/Users/me/.claude' }), existing()))
    expect(entries[0].name).toBe('Claude')
  })

  it('uses a custom name when one was given, and records it', () => {
    const detected = byApp(existing({ cliPath: '/Users/me/.claude' }), existing({ cliPath: '/Users/me/.codex' }))
    const entries = makeDefaultEntries(detected, { claude: 'Personal' })
    expect(entries[0]).toMatchObject({ name: 'Personal', customName: 'Personal' })
    expect(entries[1]).toMatchObject({ name: 'ChatGPT', customName: null })
  })

  it('carries the colours given to the defaults, and none otherwise', () => {
    const detected = byApp(existing({ guiPath: '/Applications/Claude.app' }), existing({ cliPath: '/Users/me/.codex' }))
    const entries = makeDefaultEntries(detected, {}, { claude: '#6a9bcc' })
    expect(entries[0].color).toBe('#6a9bcc')
    expect(entries[1].color).toBeNull()
  })

  it('emits only claude when codex is absent', () => {
    const entries = makeDefaultEntries(byApp(existing({ guiPath: '/Applications/Claude.app' }), existing()))
    expect(entries.map((entry) => entry.id)).toEqual(['default:claude'])
  })
})

function managed(id: string, app: AppId): SidebarEntry {
  return {
    kind: 'managed',
    profile: {
      id,
      app,
      name: id,
      slug: id,
      color: '#000000',
      createdAt: '2026-05-20T12:00:00Z',
      distinctDockIcon: false,
      lastUsedAt: null,
      surfaces: { gui: true, cli: true },
    },
  }
}

function defaultFor(app: AppId): SidebarEntry {
  return {
    kind: 'default',
    entry: { id: `default:${app}`, app, name: app, customName: null, color: null, surfaces: { gui: true, cli: true } },
  }
}

describe('groupEntriesByApp', () => {
  it('groups by app in claude→codex order, defaults alongside their managed', () => {
    const groups = groupEntriesByApp([
      defaultFor('claude'),
      managed('a', 'claude'),
      defaultFor('codex'),
      managed('b', 'codex'),
      managed('c', 'claude'),
    ])
    expect(groups.map((group) => group.app)).toEqual(['claude', 'codex'])
    expect(groups[0].default?.kind).toBe('default')
    // store order preserved within a group
    expect(groups[0].managed.map((entry) => entry.profile.id)).toEqual(['a', 'c'])
    expect(groups[1].managed.map((entry) => entry.profile.id)).toEqual(['b'])
  })

  it('omits apps with no entries and tolerates a missing default', () => {
    const groups = groupEntriesByApp([managed('a', 'claude')])
    expect(groups.map((group) => group.app)).toEqual(['claude'])
    expect(groups[0].default).toBeNull()
  })

  it('returns no groups for no entries', () => {
    expect(groupEntriesByApp([])).toEqual([])
  })
})

describe('shortcutEntries', () => {
  it('numbers rows in sidebar order: each app’s default, then its profiles', () => {
    const order = shortcutEntries([
      defaultFor('claude'),
      defaultFor('codex'),
      managed('a', 'claude'),
      managed('b', 'codex'),
      managed('c', 'claude'),
    ]).map(entryId)
    expect(order).toEqual(['default:claude', 'a', 'c', 'default:codex', 'b'])
  })

  it('stops at nine', () => {
    const many = Array.from({ length: 12 }, (_, index) => managed(`p${index}`, 'claude'))
    const order = shortcutEntries([defaultFor('claude'), ...many]).map(entryId)
    expect(order).toHaveLength(9)
    expect(order[0]).toBe('default:claude')
    expect(order[8]).toBe('p7')
  })
})
