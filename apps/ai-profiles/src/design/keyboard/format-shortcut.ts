import type { RegisterableHotkey } from '@tanstack/hotkeys'
import type { ShortcutId } from './shortcuts'

import { formatForDisplay } from '@tanstack/hotkeys'

import { shortcuts } from './shortcuts'

/**
 * Format a hotkey for chip display.
 *
 * - On macOS: returns symbol form ("⌘K", "⌥1", "⌫"). Empty separator so
 *   chips read as a single glyph cluster.
 * - On Windows/Linux: returns text form ("Ctrl+K", "Alt+1") with `+`.
 * - SSR / non-window: defaults to mac (the app's primary platform).
 */
export function formatShortcut(keys: RegisterableHotkey): string {
  const platform = detectPlatform()
  if (platform === 'mac') {
    return formatForDisplay(keys, { platform: 'mac', separatorToken: '' })
  }
  return formatForDisplay(keys, { platform })
}

function detectPlatform(): 'mac' | 'windows' | 'linux' {
  if (typeof navigator === 'undefined') {
    return 'mac'
  }
  const value = (navigator.userAgent || navigator.platform || '').toLowerCase()
  if (value.includes('mac')) {
    return 'mac'
  }
  if (value.includes('win')) {
    return 'windows'
  }
  return 'linux'
}

/**
 * A tooltip that names an action and its shortcut, "Edit (⌘E)", for buttons
 * that state their shortcut in the native help tag rather than as a chip.
 * `shortcut` is a registered id, so the text follows the real binding, or
 * keys for one bound locally (a dialog's Esc and Enter).
 */
export function shortcutHint(label: string, shortcut: ShortcutId | RegisterableHotkey): string {
  const keys = isShortcutId(shortcut) ? shortcuts[shortcut].keys : shortcut
  return `${label} (${formatShortcut(keys)})`
}

function isShortcutId(value: ShortcutId | RegisterableHotkey): value is ShortcutId {
  return typeof value === 'string' && value in shortcuts
}
