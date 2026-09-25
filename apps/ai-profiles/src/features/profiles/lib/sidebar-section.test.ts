import type { AppId } from '@/lib/app-registry'
import type { SidebarGroup } from '../api/use-sidebar-entries'
import type { ManagedEntry } from './sidebar-section'

import { describe, expect, it } from 'vitest'

import { reorderedProfileIds, visibleSection } from './sidebar-section'

/**
 * A managed profile entry for `app`, named after its id.
 */
function managed(id: string, app: AppId = 'claude', name = id): ManagedEntry {
  return {
    kind: 'managed',
    profile: {
      id,
      app,
      name,
      slug: id,
      color: '#d97757',
      createdAt: '2026-05-20T12:00:00Z',
      distinctDockIcon: false,
      lastUsedAt: null,
      surfaces: { gui: true, cli: true },
    },
  }
}

/**
 * Claude's section: its default entry, named `customName` when given, and
 * two managed profiles.
 */
function claudeGroup(customName: string | null = null): SidebarGroup {
  return {
    app: 'claude',
    default: {
      kind: 'default',
      entry: { id: 'default:claude', app: 'claude', name: 'Claude', customName, surfaces: { gui: true, cli: true } },
    },
    managed: [managed('work', 'claude', 'Work'), managed('home', 'claude', 'Home')],
  }
}

/**
 * The ids of `entries`, in order.
 */
function ids(entries: Array<ManagedEntry>): Array<string> {
  return entries.map((entry) => entry.profile.id)
}

describe('visibleSection', () => {
  it('shows every row while there is no query', () => {
    const section = visibleSection(claudeGroup(), '  ')
    expect(section.defaultRowName).toBe('Default')
    expect(section.visibleDefault?.entry.id).toBe('default:claude')
    expect(ids(section.visibleManaged)).toEqual(['work', 'home'])
  })

  it('matches names case-insensitively, ignoring surrounding space', () => {
    const section = visibleSection(claudeGroup(), ' WO ')
    expect(section.visibleDefault).toBeNull()
    expect(ids(section.visibleManaged)).toEqual(['work'])
  })

  it("matches the default row by the user's name for it", () => {
    const section = visibleSection(claudeGroup('Studio'), 'stu')
    expect(section.defaultRowName).toBe('Studio')
    expect(section.visibleDefault?.entry.id).toBe('default:claude')
    expect(section.visibleManaged).toEqual([])
  })

  it('has no default row when the app has none', () => {
    const section = visibleSection({ ...claudeGroup(), default: null }, '')
    expect(section.visibleDefault).toBeNull()
  })
})

describe('reorderedProfileIds', () => {
  const managedFlat = [managed('work'), managed('gpt', 'codex'), managed('home'), managed('side')]
  const group: SidebarGroup = {
    app: 'claude',
    default: null,
    managed: [managedFlat[0], managedFlat[2], managedFlat[3]],
  }

  it("moves the dragged profile within its app and keeps other apps' profiles in place", () => {
    expect(reorderedProfileIds(managedFlat, group, 'side', 'work')).toEqual(['side', 'gpt', 'work', 'home'])
  })

  it('moves a profile down past its neighbour', () => {
    expect(reorderedProfileIds(managedFlat, group, 'work', 'home')).toEqual(['home', 'gpt', 'work', 'side'])
  })

  it('is a no-op when dropped onto itself', () => {
    expect(reorderedProfileIds(managedFlat, group, 'home', 'home')).toBeNull()
  })

  it('is a no-op when either end is outside the section', () => {
    expect(reorderedProfileIds(managedFlat, group, 'gpt', 'work')).toBeNull()
    expect(reorderedProfileIds(managedFlat, group, 'work', 'gpt')).toBeNull()
  })
})
