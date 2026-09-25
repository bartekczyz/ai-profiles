/**
 * Which view fills the right-hand side of the window.
 */
export type RightPane = 'profile' | 'settings'

/**
 * What is on screen, as far as keyboard shortcuts care.
 */
type ShortcutGateInput = {
  /**
   * Whether an app dialog (create, edit, delete, about, what's new) is open.
   */
  dialogOpen: boolean
  /**
   * Whether the command palette is open.
   */
  paletteOpen: boolean
  /**
   * Whether the import dialog is open.
   */
  migrationOpen: boolean
  /**
   * The view on the right-hand side.
   */
  rightPane: RightPane
  /**
   * Whether a sidebar entry is selected.
   */
  hasSelection: boolean
  /**
   * Whether the selected entry is a managed profile (not a default entry).
   */
  hasManagedSelection: boolean
}

/**
 * Which shortcut groups are live.
 */
export type ShortcutGates = {
  /**
   * ⌘K — stays live while the palette itself is open so it can close it.
   */
  palette: boolean
  /**
   * Window-wide shortcuts, ⌘F and ⌘1–⌘9 — off under any overlay.
   */
  global: boolean
  /**
   * Detail-pane shortcuts (⏎, ⌘C) — need the detail pane on top of a selection.
   */
  detail: boolean
  /**
   * Edit and delete — detail shortcuts that only managed profiles support.
   */
  manageSelected: boolean
}

/**
 * Pure: global shortcuts are suppressed while a blocking overlay (dialog,
 * palette, import prompt) is on top, except the palette toggle, which must
 * keep working while the palette itself is open so ⌘K closes it.
 */
export function shortcutGates(input: ShortcutGateInput): ShortcutGates {
  const overlayOpen = input.dialogOpen || input.paletteOpen || input.migrationOpen
  const detail = input.rightPane === 'profile' && input.hasSelection && !overlayOpen
  return {
    palette: !input.dialogOpen && !input.migrationOpen,
    global: !overlayOpen,
    detail,
    manageSelected: detail && input.hasManagedSelection,
  }
}
