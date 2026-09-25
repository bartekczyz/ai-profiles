import type { ShortcutGates } from '@/features/app-shell/lib/shortcut-gates'

import { useShortcut } from '@/design'

type AppShortcutsInput = {
  /**
   * Which shortcut groups are live.
   */
  gates: ShortcutGates
  /**
   * ⌘K — opens or closes the command palette.
   */
  onTogglePalette: () => void
  /**
   * ⌘N — opens the create profile dialog.
   */
  onCreate: () => void
  /**
   * ⌘, — switches between Settings and the profile detail.
   */
  onToggleSettings: () => void
  /**
   * ⌘I — opens the import dialog for the first detected install.
   */
  onDetectImport: () => void
  /**
   * ⌘E — opens the edit dialog for the selected managed profile.
   */
  onEditSelected: () => void
  /**
   * ⌘⌫ — opens the delete dialog for the selected managed profile.
   */
  onDeleteSelected: () => void
}

/**
 * Registers the app shell's shortcuts, each gated by `gates`.
 *
 * ⏎ and ⌘C are registered by the detail pane's surfaces panel instead, so
 * the keyboard route runs the buttons' handlers verbatim (same error
 * surfacing, same copy confirmation). `gates.detail` is handed down as
 * `shortcutsEnabled` so they stay gated the same way these are.
 */
export function useAppShortcuts({
  gates,
  onTogglePalette,
  onCreate,
  onToggleSettings,
  onDetectImport,
  onEditSelected,
  onDeleteSelected,
}: AppShortcutsInput) {
  useShortcut('toggle-palette', onTogglePalette, { enabled: gates.palette })
  useShortcut('open-create-profile', onCreate, { enabled: gates.global })
  useShortcut('toggle-settings', onToggleSettings, { enabled: gates.global })
  useShortcut('open-detect-import', onDetectImport, { enabled: gates.global })
  useShortcut('edit-selected', onEditSelected, { enabled: gates.manageSelected })
  useShortcut('delete-selected', onDeleteSelected, { enabled: gates.manageSelected })
}
