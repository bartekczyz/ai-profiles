import { getShortcut, type ShortcutId } from './shortcuts'

/**
 * Format a hotkey for the `aria-keyshortcuts` HTML attribute.
 *
 * The ARIA spec uses W3C key-event names: `Control`, `Meta`, `Alt`, `Shift`
 * for modifiers, and named keys (`Backspace`, `Enter`, `ArrowDown`) or
 * characters for the key. Combinations join with `+`, multiple bindings
 * with a single space. See
 * https://www.w3.org/TR/wai-aria-1.2/#aria-keyshortcuts.
 *
 * Our hotkey strings use the platform-agnostic `Mod` token; we render that
 * as `Meta` so screen readers announce "Command" on macOS (the app's
 * primary target). On Windows/Linux some ATs will surface this as "Win
 * key" — acceptable: the same button still works under Ctrl because our
 * runtime maps Mod accordingly.
 *
 * Accepts the string form of `RegisterableHotkey` (every registered
 * shortcut in `shortcuts.ts` uses the string form). The raw-object form
 * isn't supported because none of our registrations need it.
 */
export function ariaKeyshortcuts(keys: string): string {
  return keys
    .split(' ')
    .map(formatSingleBinding)
    .filter((binding) => binding.length > 0)
    .join(' ')
}

export function ariaKeyshortcutsFor(id: ShortcutId): string {
  const { keys } = getShortcut(id)
  if (typeof keys !== 'string') {
    // Defensive: every entry in shortcuts.ts is a plain hotkey string today.
    // If someone migrates to the RawHotkey object form, fall back to the
    // empty string so we don't render `[object Object]` into the DOM.
    return ''
  }
  return ariaKeyshortcuts(keys)
}

function formatSingleBinding(binding: string): string {
  return binding
    .split('+')
    .map((token) => {
      switch (token) {
        case 'Mod':
          return 'Meta'
        case 'Ctrl':
          return 'Control'
        default:
          return token
      }
    })
    .join('+')
}
