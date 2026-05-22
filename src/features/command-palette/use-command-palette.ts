import { useState } from 'react'

export type CommandPaletteController = {
  open: boolean
  setOpen: (next: boolean) => void
  toggle: () => void
  close: () => void
}

/**
 * Tiny open/close hook. State lives at the app root so any actor (palette
 * keyboard listener, profile-detail buttons, future shortcut registry)
 * can open the palette without prop-drilling.
 */
export function useCommandPalette(): CommandPaletteController {
  const [open, setOpenState] = useState(false)
  return {
    open,
    setOpen: setOpenState,
    toggle: () => setOpenState((previous) => !previous),
    close: () => setOpenState(false),
  }
}
