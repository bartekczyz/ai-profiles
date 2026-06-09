/**
 * Profile palette — mirrors the --color-profile-* tokens in src/design/tokens.css.
 *
 * These hex values are persisted into profiles.json (Profile.color is the
 * actual swatch color, stored as data). Keeping them as literals is the
 * exception to the design-token rule: they describe a data value, not a
 * styling decision. The order matches the prototype's swatch picker.
 */
export const presetColors = [
  '#d97757', // orange
  '#7c9a6e', // sage
  '#6b8db5', // steel
  '#c19a4a', // amber
  '#c77373', // rose
  '#5c8e8e', // teal
  '#b97ab8', // mauve
  '#8b7355', // brown
] as const

export type PresetColor = (typeof presetColors)[number]

export function isValidHexColor(value: string): boolean {
  return /^#[0-9a-fA-F]{6}$/.test(value)
}
