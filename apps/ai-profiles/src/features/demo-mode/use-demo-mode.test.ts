import type { DemoEffects } from './use-demo-mode'

import { describe, expect, it, vi } from 'vitest'

import { createDemoController } from './use-demo-mode'

/**
 * Effects that record, in order, what the controller asked of them.
 */
function recordingEffects(): DemoEffects & { calls: Array<string> } {
  const calls: Array<string> = []
  return {
    calls,
    showDemoData: (on) => {
      calls.push(`data:${on}`)
    },
    applyTheme: async (theme) => {
      calls.push(`theme:${theme}`)
    },
    sizeWindow: async () => {
      calls.push('size')
    },
    restoreWindow: async () => {
      calls.push('restore')
    },
  }
}

describe('createDemoController', () => {
  it('ignores theme and size while demo data is off', async () => {
    const effects = recordingEffects()
    const controller = createDemoController(effects)

    await controller.setTheme('dark')
    await controller.sizeWindow()

    expect(effects.calls).toEqual([])
  })

  it('applies theme and size once demo data is on', async () => {
    const effects = recordingEffects()
    const controller = createDemoController(effects)

    await controller.setData(true)
    await controller.setTheme('dark')
    await controller.sizeWindow()

    expect(effects.calls).toEqual(['data:true', 'theme:dark', 'size'])
  })

  it('undoes the theme, then the window, then the data when demo data goes off', async () => {
    const effects = recordingEffects()
    const controller = createDemoController(effects)
    await controller.setData(true)
    await controller.sizeWindow()
    await controller.setTheme('light')
    effects.calls.length = 0

    await controller.setData(false)

    expect(effects.calls).toEqual(['theme:null', 'restore', 'data:false'])
  })

  it('only hides the data when nothing else was changed', async () => {
    const effects = recordingEffects()
    const controller = createDemoController(effects)
    await controller.setData(true)
    effects.calls.length = 0

    await controller.setData(false)

    expect(effects.calls).toEqual(['data:false'])
  })

  it('does nothing for a repeated switch', async () => {
    const effects = recordingEffects()
    const controller = createDemoController(effects)

    await controller.setData(false)
    await controller.setData(true)
    await controller.setData(true)

    expect(effects.calls).toEqual(['data:true'])
  })

  it('runs each request after the one before has finished', async () => {
    const effects = recordingEffects()
    let finishTheme = () => {}
    effects.applyTheme = vi.fn((theme) => {
      effects.calls.push(`theme:${theme}`)
      return theme === 'dark'
        ? new Promise<void>((resolve) => {
            finishTheme = resolve
          })
        : Promise.resolve()
    })
    const controller = createDemoController(effects)
    await controller.setData(true)

    const dark = controller.setTheme('dark')
    const off = controller.setData(false)
    await Promise.resolve()
    expect(effects.calls).toEqual(['data:true', 'theme:dark'])

    finishTheme()
    await Promise.all([dark, off])
    expect(effects.calls).toEqual(['data:true', 'theme:dark', 'theme:null', 'data:false'])
  })

  it('keeps taking requests after one fails, and still undoes it', async () => {
    const effects = recordingEffects()
    effects.sizeWindow = () => Promise.reject(new Error('no window'))
    const controller = createDemoController(effects)
    await controller.setData(true)

    await expect(controller.sizeWindow()).rejects.toThrow('no window')
    await controller.setData(false)

    expect(effects.calls).toEqual(['data:true', 'restore', 'data:false'])
  })
})
