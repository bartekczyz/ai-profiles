import { describe, expect, it } from 'vitest'

import { shortcutGates } from './shortcut-gates'

const idle = {
  dialogOpen: false,
  paletteOpen: false,
  migrationOpen: false,
  rightPane: 'profile' as const,
  hasSelection: true,
  hasManagedSelection: true,
}

describe('shortcutGates', () => {
  it('enables everything with a managed profile on screen and nothing on top', () => {
    expect(shortcutGates(idle)).toEqual({
      palette: true,
      global: true,
      detail: true,
      manageSelected: true,
    })
  })

  it('keeps only the palette toggle alive while the palette is open, so ⌘K closes it', () => {
    expect(shortcutGates({ ...idle, paletteOpen: true })).toEqual({
      palette: true,
      global: false,
      detail: false,
      manageSelected: false,
    })
  })

  it('disables everything while a dialog is open', () => {
    expect(shortcutGates({ ...idle, dialogOpen: true })).toEqual({
      palette: false,
      global: false,
      detail: false,
      manageSelected: false,
    })
  })

  it('disables everything while the import dialog is open', () => {
    expect(shortcutGates({ ...idle, migrationOpen: true })).toEqual({
      palette: false,
      global: false,
      detail: false,
      manageSelected: false,
    })
  })

  it('gates detail shortcuts off while Settings covers the detail pane', () => {
    expect(shortcutGates({ ...idle, rightPane: 'settings' })).toMatchObject({
      global: true,
      detail: false,
      manageSelected: false,
    })
  })

  it('gates detail shortcuts off with nothing selected', () => {
    expect(shortcutGates({ ...idle, hasSelection: false, hasManagedSelection: false })).toMatchObject({
      detail: false,
      manageSelected: false,
    })
  })

  it('keeps detail shortcuts but not edit/delete for a default entry', () => {
    expect(shortcutGates({ ...idle, hasManagedSelection: false })).toMatchObject({
      detail: true,
      manageSelected: false,
    })
  })
})
