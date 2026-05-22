import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { formatShortcut } from './format-shortcut'
import { getShortcut, type ShortcutId, shortcuts } from './shortcuts'

const shortcutIds = Object.keys(shortcuts) as Array<ShortcutId>

describe('shortcut registry', () => {
  it('every entry has keys and complete meta', () => {
    for (const id of shortcutIds) {
      const definition = getShortcut(id)
      expect(definition.keys, `${id} keys`).toBeTruthy()
      expect(definition.meta.name, `${id} meta.name`).toBeTruthy()
      expect(definition.meta.description, `${id} meta.description`).toBeTruthy()
      expect(definition.meta.scope, `${id} meta.scope`).toMatch(/^(global|detail|palette|modal)$/)
    }
  })

  it('keys are unique across the registry', () => {
    const seen = new Set<string>()
    for (const id of shortcutIds) {
      const keys = String(getShortcut(id).keys)
      expect(seen.has(keys), `duplicate keys: ${keys}`).toBe(false)
      seen.add(keys)
    }
  })
})

// formatShortcut auto-detects the platform from navigator.userAgent.
// Stub it per test so we can confirm both the macOS symbol form and the
// Windows/Linux text form work.
describe('formatShortcut', () => {
  const originalUserAgent = Object.getOwnPropertyDescriptor(globalThis.navigator, 'userAgent')

  beforeEach(() => {
    Object.defineProperty(globalThis.navigator, 'userAgent', {
      value: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)',
      configurable: true,
    })
  })

  afterEach(() => {
    if (originalUserAgent) {
      Object.defineProperty(globalThis.navigator, 'userAgent', originalUserAgent)
    }
    vi.restoreAllMocks()
  })

  it('uses ⌘ symbol for Mod+K on Mac', () => {
    expect(formatShortcut('Mod+K')).toMatch(/⌘.*K/)
  })

  it('uses ⌥ symbol for Alt+1 on Mac', () => {
    expect(formatShortcut('Alt+1')).toMatch(/⌥.*1/)
  })

  it('renders Ctrl+K on Windows', () => {
    Object.defineProperty(globalThis.navigator, 'userAgent', {
      value: 'Mozilla/5.0 (Windows NT 10.0)',
      configurable: true,
    })
    expect(formatShortcut('Mod+K')).toMatch(/Ctrl/)
  })

  it('renders single-key shortcuts', () => {
    expect(formatShortcut('Enter')).toBeTruthy()
    expect(formatShortcut('Escape')).toBeTruthy()
  })
})
