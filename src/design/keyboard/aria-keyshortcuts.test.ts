import { describe, expect, it } from 'vitest'

import { ariaKeyshortcuts, ariaKeyshortcutsFor } from './aria-keyshortcuts'

describe('ariaKeyshortcuts', () => {
  it('rewrites the platform-agnostic Mod token to Meta (W3C key-event name)', () => {
    expect(ariaKeyshortcuts('Mod+N')).toBe('Meta+N')
    expect(ariaKeyshortcuts('Mod+Shift+K')).toBe('Meta+Shift+K')
  })

  it('passes through ARIA-spec modifiers untouched', () => {
    expect(ariaKeyshortcuts('Alt+1')).toBe('Alt+1')
    expect(ariaKeyshortcuts('Shift+Enter')).toBe('Shift+Enter')
  })

  it('rewrites Ctrl to Control (matches the W3C name)', () => {
    expect(ariaKeyshortcuts('Ctrl+S')).toBe('Control+S')
  })

  it('preserves named keys like Backspace, ArrowDown, Enter', () => {
    expect(ariaKeyshortcuts('Mod+Backspace')).toBe('Meta+Backspace')
    expect(ariaKeyshortcuts('Enter')).toBe('Enter')
  })

  it('joins multiple bindings with a single space (ARIA spec separator)', () => {
    expect(ariaKeyshortcuts('Mod+K Mod+P')).toBe('Meta+K Meta+P')
  })
})

describe('ariaKeyshortcutsFor', () => {
  it('resolves a registered shortcut id and formats its keys', () => {
    expect(ariaKeyshortcutsFor('open-create-profile')).toBe('Meta+N')
    expect(ariaKeyshortcutsFor('toggle-palette')).toBe('Meta+K')
    expect(ariaKeyshortcutsFor('reveal-gui-data')).toBe('Alt+1')
    expect(ariaKeyshortcutsFor('open-selected-desktop')).toBe('Enter')
    expect(ariaKeyshortcutsFor('delete-selected')).toBe('Meta+Backspace')
  })
})
