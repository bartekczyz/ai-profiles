import type { AppId } from '@/lib/app-registry'
import type { ExistingInstallInfo } from '@/lib/types'

import { describe, expect, it } from 'vitest'

import { makeDefaultEntries } from './use-sidebar-entries'

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

  it('emits only claude when codex is absent', () => {
    const entries = makeDefaultEntries(byApp(existing({ guiPath: '/Applications/Claude.app' }), existing()))
    expect(entries.map((entry) => entry.id)).toEqual(['default:claude'])
  })
})
