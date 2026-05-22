import type { RegisterableHotkey } from '@tanstack/hotkeys'

/**
 * Single source of truth for keyboard shortcuts. Every `<Kbd shortcutId>`
 * chip in the UI reads from this map, and every `useShortcut(id, …)`
 * binding pulls the keys + metadata from here too. Renaming a binding is
 * a one-line change.
 *
 * Augment `HotkeyMeta` (in @tanstack/hotkeys) to add the `scope` field —
 * see ./scope-meta.d.ts.
 */

export type Scope = 'global' | 'detail' | 'palette' | 'modal'

export type ShortcutDefinition = {
  /** Hotkey string in canonical form (Mod = ⌘ on Mac / Ctrl elsewhere). */
  keys: RegisterableHotkey
  meta: {
    name: string
    description: string
    scope: Scope
  }
}

export const shortcuts = {
  'toggle-palette': {
    keys: 'Mod+K',
    meta: { name: 'Command palette', description: 'Open the command palette', scope: 'global' },
  },
  'open-create-profile': {
    keys: 'Mod+N',
    meta: { name: 'New profile', description: 'Open the create-profile dialog', scope: 'global' },
  },
  'toggle-settings': {
    keys: 'Mod+,',
    meta: { name: 'Toggle settings', description: 'Open or close the Settings pane', scope: 'global' },
  },
  'open-detect-import': {
    keys: 'Mod+I',
    meta: { name: 'Detect and import', description: 'Open the migration dialog', scope: 'global' },
  },
  'edit-selected': {
    keys: 'Mod+E',
    meta: { name: 'Edit profile', description: 'Edit the currently selected profile', scope: 'detail' },
  },
  'copy-selected-cli': {
    keys: 'Mod+C',
    meta: { name: 'Copy CLI command', description: 'Copy claude-{slug} to the clipboard', scope: 'detail' },
  },
  'delete-selected': {
    keys: 'Mod+Backspace',
    meta: { name: 'Delete profile', description: 'Open the delete confirmation dialog', scope: 'detail' },
  },
  'open-selected-desktop': {
    keys: 'Enter',
    meta: { name: 'Open desktop app', description: 'Launch Claude Desktop for the active profile', scope: 'detail' },
  },
  'reveal-gui-data': {
    keys: 'Alt+1',
    meta: { name: 'Reveal data', description: 'Open the GUI data directory in Finder', scope: 'detail' },
  },
  'reveal-gui-launcher': {
    keys: 'Alt+2',
    meta: { name: 'Reveal launcher', description: 'Open the .app launcher in Finder', scope: 'detail' },
  },
  'reveal-cli-config': {
    keys: 'Alt+3',
    meta: { name: 'Reveal CLI config', description: 'Open the CLI config dir in Finder', scope: 'detail' },
  },
  'reveal-cli-wrapper': {
    keys: 'Alt+4',
    meta: { name: 'Reveal wrapper', description: 'Open the CLI wrapper script in Finder', scope: 'detail' },
  },
} as const satisfies Record<string, ShortcutDefinition>

export type ShortcutId = keyof typeof shortcuts

export function getShortcut(id: ShortcutId): ShortcutDefinition {
  return shortcuts[id]
}
