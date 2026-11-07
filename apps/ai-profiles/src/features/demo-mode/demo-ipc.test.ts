import { invoke } from '@tauri-apps/api/core'
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'

import { archiveSession, listProfiles, listSessions, loadAppState, updateAppState } from '@/lib/commands'

import { createDemoInvoke, installDemoIpc, routeCommand, setDemoEnabled } from './demo-ipc'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const mockInvoke = vi.mocked(invoke)

function handles(command: string): boolean {
  return command === 'list_profiles'
}

describe('routeCommand', () => {
  it('sends everything to the backend while demo data is off', () => {
    expect(routeCommand('list_profiles', false, handles)).toBe('backend')
    expect(routeCommand('no_such_command', false, handles)).toBe('backend')
  })

  it('answers an app command it has a handler for from the fixtures', () => {
    expect(routeCommand('list_profiles', true, handles)).toBe('fixture')
  })

  it('refuses an app command it has no handler for', () => {
    expect(routeCommand('no_such_command', true, handles)).toBe('reject')
  })

  it('lets plugin and dev tool commands through', () => {
    expect(routeCommand('plugin:window|set_size', true, handles)).toBe('backend')
    expect(routeCommand('dev_set_window_theme', true, handles)).toBe('backend')
  })
})

describe('demo IPC', () => {
  beforeAll(() => {
    installDemoIpc()
  })

  beforeEach(() => {
    mockInvoke.mockReset()
    mockInvoke.mockResolvedValue([])
  })

  afterEach(() => {
    setDemoEnabled(false)
    vi.useRealTimers()
  })

  it('passes commands through untouched while off', async () => {
    await listProfiles()

    expect(mockInvoke).toHaveBeenCalledWith('list_profiles', undefined, undefined)
  })

  it('answers app commands without reaching the backend while on', async () => {
    setDemoEnabled(true)

    const profiles = await listProfiles()

    expect(profiles.length).toBeGreaterThan(0)
    expect(mockInvoke).not.toHaveBeenCalled()
  })

  it('rejects an app command it has no answer for, without reaching the backend', async () => {
    setDemoEnabled(true)
    const demoInvoke = createDemoInvoke(mockInvoke)

    await expect(demoInvoke('no_such_command')).rejects.toMatchObject({ kind: 'Validation' })
    expect(mockInvoke).not.toHaveBeenCalled()
  })

  it('passes plugin commands through while on', async () => {
    setDemoEnabled(true)
    const demoInvoke = createDemoInvoke(mockInvoke)

    await demoInvoke('plugin:app|version')

    expect(mockInvoke).toHaveBeenCalledWith('plugin:app|version', undefined, undefined)
  })

  it('reads back what a write changed', async () => {
    setDemoEnabled(true)

    await updateAppState({ selectedEntryId: 'default:codex' })

    expect((await loadAppState()).selectedEntryId).toBe('default:codex')
  })

  it('archives a session so the next listing has it archived', async () => {
    setDemoEnabled(true)
    const { selectedEntryId } = await loadAppState()
    const owner = selectedEntryId ?? ''
    const active = (await listSessions(owner)).sessions.find((session) => !session.archived)

    await archiveSession(owner, active?.id ?? '', false)

    const after = (await listSessions(owner)).sessions.find((session) => session.id === active?.id)
    expect(after?.archived).toBe(true)
  })

  it('starts afresh each time it is turned on', async () => {
    setDemoEnabled(true)
    await updateAppState({ selectedEntryId: 'default:codex' })

    setDemoEnabled(false)
    setDemoEnabled(true)

    expect((await loadAppState()).selectedEntryId).not.toBe('default:codex')
  })

  it('keeps every age as it was, however much later it is read', async () => {
    vi.useFakeTimers({ toFake: ['Date'] })
    setDemoEnabled(true)
    const ageNow = Date.now() - Date.parse((await listProfiles())[0].lastUsedAt ?? '')

    vi.setSystemTime(Date.now() + 3 * 60 * 60 * 1000)

    const ageLater = Date.now() - Date.parse((await listProfiles())[0].lastUsedAt ?? '')
    expect(ageLater).toBe(ageNow)
  })
})
