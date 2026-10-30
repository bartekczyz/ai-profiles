import type { HotkeyCallback } from '@tanstack/hotkeys'

import { useHotkey } from '@tanstack/react-hotkeys'

import { getShortcut, type ShortcutId } from './shortcuts'

type Options = {
  /** Soft-disable: the registration stays but the callback won't fire. */
  enabled?: boolean
}

/**
 * Bind a shortcut by id. Pulls the key string + metadata from the central
 * registry so renaming a binding (e.g. `Mod+K` → `Mod+Shift+K`) updates
 * both the listener AND every `<Kbd shortcutId>` chip in the UI in lock
 * step.
 */
export function useShortcut(id: ShortcutId, callback: HotkeyCallback, options: Options = {}): void {
  const definition = getShortcut(id)
  useHotkey(definition.keys, callback, {
    enabled: options.enabled,
    // Shortcuts that act on the selected profile stand down while a text
    // field has focus, where the same keys edit text: ⌘⌫ deletes to the start
    // of the line, ⌘C copies the selection. App-wide ones (⌘K, ⌘F, ⌘N) still
    // fire from a field, as they do in native apps.
    ignoreInputs: definition.meta.scope === 'detail' ? true : undefined,
    meta: definition.meta,
  })
}
