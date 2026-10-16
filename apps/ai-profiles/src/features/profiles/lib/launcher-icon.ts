/**
 * A change to a profile's launcher icon, made in the edit form and applied on
 * save: a PNG of the user's to use, with the data URL it is previewed from, or
 * a return to the generated icon.
 */
export type IconChange = { kind: 'custom'; file: File; preview: string } | { kind: 'generated' }

/**
 * How the launcher-icon controls of the edit form read.
 */
type LauncherIconControls = {
  /**
   * The icon the launcher has, or will have once the form is saved, as a data
   * URL; `null` while it is still being rendered.
   */
  icon: string | null
  /**
   * Whether there is an icon of the user's to remove.
   */
  canRemove: boolean
}

/**
 * Pure: how the launcher-icon controls read, given the profile's own icon
 * (`null` for none, `undefined` until that is known), the change made in the
 * form, if any, and the generated icon. A change shows at once, though it is
 * applied only on save. Until the profile's own icon is known nothing is
 * shown, rather than the generated icon it may not have.
 */
export function launcherIconControls(
  customIcon: string | null | undefined,
  change: IconChange | null,
  generated: string | null,
): LauncherIconControls {
  if (change?.kind === 'custom') {
    return { icon: change.preview, canRemove: true }
  }
  if (change === null && customIcon === undefined) {
    return { icon: null, canRemove: false }
  }
  if (change === null && customIcon != null) {
    return { icon: customIcon, canRemove: true }
  }
  return { icon: generated, canRemove: false }
}

/**
 * Pure: the change that removes the user's icon. A profile with no icon of its
 * own has the generated one already, so all there is to do is drop an image
 * picked in the form.
 */
export function resetIcon(isSet: boolean): IconChange | null {
  return isSet ? { kind: 'generated' } : null
}

/**
 * Pure: `png` as a data URL an `<img>` can show (the window's content policy
 * allows `data:` images, not `blob:` ones), or `null` for no bytes at all.
 */
export function pngDataUrl(png: ArrayBuffer): string | null {
  const bytes = new Uint8Array(png)
  if (bytes.length === 0) {
    return null
  }
  // In chunks: spreading a large image into one call overflows the stack.
  let binary = ''
  for (let start = 0; start < bytes.length; start += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(start, start + 0x8000))
  }
  return `data:image/png;base64,${btoa(binary)}`
}
