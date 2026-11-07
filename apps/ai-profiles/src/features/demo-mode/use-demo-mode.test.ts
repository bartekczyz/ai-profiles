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
    waitUntilRendered: async () => {
      calls.push('wait')
    },
    captureWindow: async (theme) => {
      calls.push(`capture:${theme}`)
      return `screenshot-${theme}.png`
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

  it('captures light then dark on demo data in the screenshot size, then puts the real app back', async () => {
    const effects = recordingEffects()
    const controller = createDemoController(effects)

    const saved = await controller.capture()

    expect(effects.calls).toEqual([
      'data:true',
      'size',
      'theme:light',
      'wait',
      'capture:light',
      'theme:dark',
      'wait',
      'capture:dark',
      'theme:null',
      'restore',
      'data:false',
    ])
    expect(saved).toEqual(['screenshot-light.png', 'screenshot-dark.png'])
  })

  it('puts everything back when a capture fails', async () => {
    const effects = recordingEffects()
    effects.captureWindow = (theme) =>
      theme === 'dark' ? Promise.reject(new Error('no permission')) : Promise.resolve('screenshot-light.png')
    const controller = createDemoController(effects)

    await expect(controller.capture()).rejects.toThrow('no permission')

    expect(effects.calls.slice(-3)).toEqual(['theme:null', 'restore', 'data:false'])
  })

  it('leaves a demo that was already on as it was after capturing', async () => {
    const effects = recordingEffects()
    const controller = createDemoController(effects)
    await controller.setData(true)
    await controller.setTheme('dark')
    effects.calls.length = 0

    await controller.capture()

    expect(effects.calls).toEqual([
      'size',
      'theme:light',
      'wait',
      'capture:light',
      'theme:dark',
      'wait',
      'capture:dark',
      'restore',
    ])
  })
})
