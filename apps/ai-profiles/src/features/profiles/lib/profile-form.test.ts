import type { Dependencies } from '@/lib/types'

import { describe, expect, it } from 'vitest'

import { appSpecs } from '@/lib/app-registry'

import {
  availableSurfaces,
  dockIconDescription,
  effectiveSurfaces,
  installedAppIds,
  isProfileFormValid,
  newProfileDockIcon,
  preselectedApp,
  surfaceToggle,
} from './profile-form'

const dependencies: Dependencies = {
  apps: {
    claude: { guiInstalled: true, cliInstalled: false },
    codex: { guiInstalled: false, cliInstalled: false },
  },
  localBinOnPath: true,
}

describe('installedAppIds', () => {
  it('lists the apps with either surface installed', () => {
    expect(installedAppIds(dependencies)).toEqual(['claude'])
  })

  it('counts an app with only its CLI installed', () => {
    const cliOnly: Dependencies = {
      ...dependencies,
      apps: { ...dependencies.apps, codex: { guiInstalled: false, cliInstalled: true } },
    }
    expect(installedAppIds(cliOnly)).toEqual(['claude', 'codex'])
  })
})

describe('preselectedApp', () => {
  it('picks the only installed app', () => {
    expect(preselectedApp(['codex'])).toBe('codex')
  })

  it('leaves the choice to the user when several apps are installed', () => {
    expect(preselectedApp(['claude', 'codex'])).toBe('')
  })

  it('leaves it empty when no app is installed', () => {
    expect(preselectedApp([])).toBe('')
  })
})

describe('availableSurfaces', () => {
  it("mirrors the app's installed surfaces", () => {
    expect(availableSurfaces(dependencies, 'claude')).toEqual({ gui: true, cli: false })
  })

  it('offers nothing until an app is chosen', () => {
    expect(availableSurfaces(dependencies, '')).toEqual({ gui: false, cli: false })
  })
})

describe('effectiveSurfaces', () => {
  it('keeps only the chosen surfaces that are available', () => {
    expect(effectiveSurfaces({ gui: true, cli: true }, { gui: true, cli: false })).toEqual({ gui: true, cli: false })
  })

  it('does not switch on an available surface the user left off', () => {
    expect(effectiveSurfaces({ gui: false, cli: true }, { gui: true, cli: true })).toEqual({ gui: false, cli: true })
  })
})

describe('isProfileFormValid', () => {
  const surfaces = { gui: true, cli: false }

  it('accepts a name, a valid color and a surface', () => {
    expect(isProfileFormValid('Work', '#aabbcc', surfaces)).toBe(true)
  })

  it('rejects a blank name', () => {
    expect(isProfileFormValid('   ', '#aabbcc', surfaces)).toBe(false)
  })

  it('rejects an invalid color', () => {
    expect(isProfileFormValid('Work', '#zzz', surfaces)).toBe(false)
  })

  it('rejects a profile without any surface', () => {
    expect(isProfileFormValid('Work', '#aabbcc', { gui: false, cli: false })).toBe(false)
  })
})

describe('newProfileDockIcon', () => {
  it('starts off until the user has acknowledged what it involves', () => {
    const claude = appSpecs.claude.dockIcon
    appSpecs.claude.dockIcon = { defaultOn: true, cost: null }
    try {
      expect(newProfileDockIcon(null, 'claude', false, true)).toBe(false)
      expect(newProfileDockIcon(null, 'claude', true, true)).toBe(true)
    } finally {
      appSpecs.claude.dockIcon = claude
    }
  })

  it("follows the app's default once acknowledged", () => {
    expect(newProfileDockIcon(null, 'codex', true, true)).toBe(appSpecs.codex.dockIcon.defaultOn)
  })

  it('starts off while no app is chosen', () => {
    expect(newProfileDockIcon(null, '', true, true)).toBe(false)
  })

  it("follows the user's choice over the default", () => {
    expect(newProfileDockIcon(true, 'codex', false, true)).toBe(true)
    expect(newProfileDockIcon(false, 'codex', true, true)).toBe(false)
  })

  it('is off without a desktop launcher, whatever the user chose', () => {
    expect(newProfileDockIcon(true, 'claude', true, false)).toBe(false)
  })
})

describe('dockIconDescription', () => {
  it("names only the chosen app's cost", () => {
    const description = dockIconDescription('codex')
    expect(description).toContain(appSpecs.codex.dockIcon.cost)
    expect(description).not.toContain(appSpecs.claude.dockIcon.cost)
  })

  it('names no cost for an app that has none', () => {
    const codex = appSpecs.codex.dockIcon
    appSpecs.codex.dockIcon = { defaultOn: false, cost: null }
    try {
      expect(dockIconDescription('codex')).not.toContain('null')
    } finally {
      appSpecs.codex.dockIcon = codex
    }
  })

  it('names no cost while no app is chosen', () => {
    const description = dockIconDescription('')
    expect(description).not.toContain(appSpecs.claude.dockIcon.cost)
    expect(description).not.toContain(appSpecs.codex.dockIcon.cost)
  })
})

describe('surfaceToggle', () => {
  it("is checked when chosen and available, with the app's copy", () => {
    expect(surfaceToggle('gui', 'claude', true, true)).toEqual({
      checked: true,
      disabled: false,
      title: appSpecs.claude.gui.label,
      description: appSpecs.claude.gui.description,
      install: null,
    })
  })

  it('stays unchecked when available but not chosen', () => {
    expect(surfaceToggle('cli', 'claude', true, false)).toMatchObject({ checked: false, disabled: false })
  })

  it('is disabled and unchecked when unavailable, linking to the desktop app install', () => {
    expect(surfaceToggle('gui', 'codex', false, true)).toMatchObject({
      checked: false,
      disabled: true,
      install: { href: appSpecs.codex.gui.installUrl, label: `${appSpecs.codex.displayName} Desktop` },
    })
  })

  it('links a missing CLI to its own install page, under its own name', () => {
    expect(surfaceToggle('cli', 'codex', false, true).install).toEqual({
      href: appSpecs.codex.cli.installUrl,
      label: appSpecs.codex.cliDisplayName,
    })
  })

  it('is disabled without an install link while no app is chosen', () => {
    const toggle = surfaceToggle('cli', '', false, true)
    expect(toggle).toMatchObject({ checked: false, disabled: true, description: '', install: null })
    expect(toggle.title).not.toBe('')
  })
})
