import type { DefaultEntry } from '@/lib/types'

import { isValidHexColor } from '@/lib/colors'

/**
 * What changed. A field left out is left as it is; an empty string puts the
 * stock label, or no colour, back.
 */
export type DefaultProfileEdit = {
  /**
   * The new name, or `''` for the stock label.
   */
  name?: string
  /**
   * The new colour as `#rrggbb`, or `''` for none.
   */
  color?: string
}

/**
 * What saving the Edit default profile form would do.
 */
type PlannedEdit = {
  /**
   * The fields that changed, as typed and tidied.
   */
  edit: DefaultProfileEdit
  /**
   * Whether it can be saved: something changed, and the colour is one.
   */
  valid: boolean
}

/**
 * Pure: the edit the form's `name` and `color` make to `entry`. Both are
 * trimmed and the colour lowercased, as the Rust side stores them.
 */
export function planDefaultProfileEdit(entry: DefaultEntry, name: string, color: string): PlannedEdit {
  const trimmedName = name.trim()
  const trimmedColor = color.trim().toLowerCase()
  const edit: DefaultProfileEdit = {
    ...(trimmedName === (entry.customName ?? '') ? {} : { name: trimmedName }),
    ...(trimmedColor === (entry.color ?? '') ? {} : { color: trimmedColor }),
  }
  const colorValid = trimmedColor === '' || isValidHexColor(trimmedColor)
  return { edit, valid: Object.keys(edit).length > 0 && colorValid }
}

/**
 * Pure: the edit that puts `entry` back to its stock label and no colour,
 * touching only what was customised. Empty when nothing was.
 */
export function resetDefaultProfileEdit(entry: DefaultEntry): DefaultProfileEdit {
  return {
    ...(entry.customName === null ? {} : { name: '' }),
    ...(entry.color === null ? {} : { color: '' }),
  }
}
