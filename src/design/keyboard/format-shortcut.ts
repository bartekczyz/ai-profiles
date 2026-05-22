import type { RegisterableHotkey } from '@tanstack/hotkeys'

import { formatForDisplay } from '@tanstack/hotkeys'

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
