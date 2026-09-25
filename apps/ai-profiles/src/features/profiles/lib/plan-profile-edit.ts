import type { Profile, ProfilePatch, Surface, Surfaces } from '@/lib/types'

/**
 * What the edit dialog submits.
 */
export type ProfileEditInput = {
  /**
   * The profile's name as typed.
   */
  name: string
  /**
   * The chosen swatch color.
   */
  color: string
  /**
   * Which launchers the profile should have.
   */
  surfaces: Surfaces
  /**
   * Whether the desktop launcher should carry a Dock icon of its own.
   */
  distinctDockIcon: boolean
}

/**
 * The backend calls an edit needs, in the order they run.
 */
type ProfileEditPlan = {
  /**
   * The profile update to send, or `null` when name, color and Dock icon are unchanged.
   */
  patch: ProfilePatch | null
  /**
   * The surfaces to switch on or off, gui before cli.
   */
  toggles: Array<{ surface: Surface; enabled: boolean }>
}

const surfaceOrder: Array<Surface> = ['gui', 'cli']

/**
 * Pure: diffs the edit form against the saved profile. Name and color are
 * always sent together when anything in the patch changed; the Dock icon only
 * when it flipped, since switching it rebuilds the launcher. Color compares
 * case-insensitively so re-picking the same swatch is a no-op.
 */
export function planProfileEdit(profile: Profile, input: ProfileEditInput): ProfileEditPlan {
  const nameChanged = input.name !== profile.name
  const colorChanged = input.color.toLowerCase() !== profile.color.toLowerCase()
  const dockIconChanged = input.distinctDockIcon !== profile.distinctDockIcon
  const patch =
    nameChanged || colorChanged || dockIconChanged
      ? {
          name: input.name,
          color: input.color,
          ...(dockIconChanged ? { distinctDockIcon: input.distinctDockIcon } : {}),
        }
      : null
  const toggles = surfaceOrder
    .filter((surface) => input.surfaces[surface] !== profile.surfaces[surface])
    .map((surface) => ({ surface, enabled: input.surfaces[surface] }))
  return { patch, toggles }
}
