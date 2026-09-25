import type { AppId, AppSpec } from '@/lib/app-registry'
import type { Dependencies, Surface, Surfaces } from '@/lib/types'

import { appIds, appSpecs } from '@/lib/app-registry'
import { isValidHexColor } from '@/lib/colors'

/**
 * The app a profile form is for, or `''` while none has been chosen.
 */
type FormApp = AppId | ''

/**
 * Where to get a surface that is not installed yet.
 */
type SurfaceInstall = {
  /**
   * The download page.
   */
  href: string
  /**
   * The product to install, as the link reads.
   */
  label: string
}

/**
 * How a surface's toggle reads in the profile form.
 */
export type SurfaceToggle = {
  /**
   * Whether the toggle shows as on: chosen, and installed.
   */
  checked: boolean
  /**
   * Whether the toggle is unavailable, because the surface is not installed.
   */
  disabled: boolean
  /**
   * The toggle's title.
   */
  title: string
  /**
   * The line under the title.
   */
  description: string
  /**
   * Where to install the surface when it is missing, or `null` when there is
   * nothing to install.
   */
  install: SurfaceInstall | null
}

const fallbackTitles: Record<Surface, string> = { gui: 'Desktop App launcher', cli: 'CLI wrapper' }

/**
 * Pure: the apps with at least one surface installed, in registry order.
 */
export function installedAppIds(dependencies: Dependencies): Array<AppId> {
  return appIds.filter((id) => dependencies.apps[id].guiInstalled || dependencies.apps[id].cliInstalled)
}

/**
 * Pure: the app to pre-select when exactly one is installed; otherwise empty,
 * so the user makes a deliberate choice.
 */
export function preselectedApp(installedApps: ReadonlyArray<AppId>): FormApp {
  return installedApps.length === 1 ? installedApps[0] : ''
}

/**
 * Pure: which surfaces of the chosen app are installed. None are before an app
 * is chosen.
 */
export function availableSurfaces(dependencies: Dependencies, app: FormApp): Surfaces {
  if (app === '') {
    return { gui: false, cli: false }
  }
  const installed = dependencies.apps[app]
  return { gui: installed.guiInstalled, cli: installed.cliInstalled }
}

/**
 * Pure: the surfaces a profile will get — those chosen that are also available.
 */
export function effectiveSurfaces(chosen: Surfaces, available: Surfaces): Surfaces {
  return { gui: chosen.gui && available.gui, cli: chosen.cli && available.cli }
}

/**
 * Pure: whether the form holds a profile that can be saved — a name, a valid
 * color, and at least one surface.
 */
export function isProfileFormValid(name: string, color: string, surfaces: Surfaces): boolean {
  return name.trim().length > 0 && isValidHexColor(color) && (surfaces.gui || surfaces.cli)
}

/**
 * Pure: whether a new profile gets a Dock icon of its own. The user's choice
 * wins once they have made one; until then it starts on only once they have
 * acknowledged what that involves, and only for an app where it costs nothing
 * they would notice. Never without a desktop launcher, which it belongs to.
 */
export function newProfileDockIcon(
  choice: boolean | null,
  app: FormApp,
  acknowledged: boolean,
  desktopLauncher: boolean,
): boolean {
  const byDefault = app !== '' && acknowledged && appSpecs[app].dockIcon.defaultOn
  return (choice ?? byDefault) && desktopLauncher
}

/**
 * Pure: what the Dock icon option says next to its checkbox, so the price of it
 * (for ChatGPT, its notifications) is in front of the user before they turn it
 * on, not only in the explanation that follows.
 */
export function dockIconDescription(app: FormApp): string {
  const intro = 'Gives this profile its own Dock icon and name.'
  if (app === '') {
    return intro
  }
  const spec = appSpecs[app]
  const cost = spec.dockIcon.cost === null ? '' : ` ${spec.dockIcon.cost}`
  return `${intro}${cost} The default ${spec.displayName} keeps the stock app.`
}

/**
 * Pure: how a surface's toggle reads. It self-disables when the surface is not
 * installed, and then says where to get it.
 */
export function surfaceToggle(surface: Surface, app: FormApp, available: boolean, chosen: boolean): SurfaceToggle {
  const state = { checked: chosen && available, disabled: !available }
  if (app === '') {
    return { ...state, title: fallbackTitles[surface], description: '', install: null }
  }
  const spec = appSpecs[app]
  return {
    ...state,
    title: spec[surface].label,
    description: spec[surface].description,
    install: available ? null : { href: spec[surface].installUrl, label: installLabel(surface, spec) },
  }
}

/**
 * The product a missing surface's install link names.
 */
function installLabel(surface: Surface, spec: AppSpec): string {
  return surface === 'gui' ? `${spec.displayName} Desktop` : spec.cliDisplayName
}
