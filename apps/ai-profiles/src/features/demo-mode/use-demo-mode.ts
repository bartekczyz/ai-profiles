import type { QueryClient } from '@tanstack/react-query'
import type { DevTheme } from '@/lib/types'

import { useEffect, useEffectEvent, useState } from 'react'

import { listen } from '@tauri-apps/api/event'

import { useTheme, useToast } from '@/design'
import { devCaptureWindow, devPrepareScreenshotWindow, devRestoreWindow, devSetWindowTheme } from '@/lib/commands'
import { extractErrorMessage } from '@/lib/extract-error-message'
import { createQueryClient } from '@/lib/query/client'

import { setDemoEnabled } from './demo-ipc'

/**
 * What the Developer menu has changed, which ending the demo undoes.
 */
type DemoSession = {
  /**
   * Whether the app shows demo data.
   */
  data: boolean
  /**
   * The theme the app and its window are forced into, or `null` for the
   * user's own.
   */
  theme: DevTheme | null
  /**
   * Whether the window has the screenshot size.
   */
  sized: boolean
}

const realApp: DemoSession = { data: false, theme: null, sized: false }

const screenshotThemes: ReadonlyArray<DevTheme> = ['light', 'dark']

/**
 * How long a theme change takes to reach the window's material, which the
 * webview's frames don't tell.
 */
const materialSettleMs = 300

/**
 * How the demo changes the app and its window.
 */
export type DemoEffects = {
  /**
   * Shows demo data, or the real data again.
   */
  showDemoData: (on: boolean) => void
  /**
   * Forces the app and its window into `theme`, or lifts the force with
   * `null`.
   */
  applyTheme: (theme: DevTheme | null) => Promise<void>
  /**
   * Gives the window the screenshot size, remembering where it was.
   */
  sizeWindow: () => Promise<void>
  /**
   * Puts the window back where `sizeWindow` found it.
   */
  restoreWindow: () => Promise<void>
  /**
   * Resolves once the app shows all it is going to.
   */
  waitUntilRendered: () => Promise<void>
  /**
   * Saves the window as the screenshot in `theme`, resolving with where.
   */
  captureWindow: (theme: DevTheme) => Promise<string>
}

/**
 * What the Developer menu asks of the demo. Each request waits for the ones
 * before it to finish.
 */
type DemoController = {
  /**
   * Turns demo data on, or off, undoing everything else the demo changed.
   */
  setData: (on: boolean) => Promise<void>
  /**
   * Forces the app and its window into `theme`, while demo data is on.
   */
  setTheme: (theme: DevTheme) => Promise<void>
  /**
   * Gives the window the screenshot size, while demo data is on.
   */
  sizeWindow: () => Promise<void>
  /**
   * Saves the screenshots, light then dark, of demo data in the screenshot
   * size, then puts everything back as it was, even when one fails. Resolves
   * with where they went.
   */
  capture: () => Promise<Array<string>>
}

/**
 * The demo's state machine, over `effects`: theme and size only apply while
 * demo data is on, and turning it off puts the theme, then the window, then
 * the real data back.
 */
export function createDemoController(effects: DemoEffects): DemoController {
  let session = realApp
  let queue: Promise<unknown> = Promise.resolve()

  function serially<T>(step: () => Promise<T>): Promise<T> {
    const run = queue.then(step)
    queue = run.catch(() => undefined)
    return run
  }

  function showData() {
    if (!session.data) {
      effects.showDemoData(true)
      session = { ...session, data: true }
    }
  }

  // The two below note the change first, so a half-done one is still undone.
  async function applyTheme(theme: DevTheme) {
    session = { ...session, theme }
    await effects.applyTheme(theme)
  }

  async function sizeWindow() {
    session = { ...session, sized: true }
    await effects.sizeWindow()
  }

  async function restore(target: DemoSession) {
    if (session.theme !== target.theme) {
      await effects.applyTheme(target.theme)
      session = { ...session, theme: target.theme }
    }
    if (session.sized && !target.sized) {
      await effects.restoreWindow()
      session = { ...session, sized: false }
    }
    if (session.data && !target.data) {
      effects.showDemoData(false)
      session = { ...session, data: false }
    }
  }

  return {
    setData: (on) =>
      serially(async () => {
        if (on) {
          showData()
        } else {
          await restore(realApp)
        }
      }),
    setTheme: (theme) =>
      serially(async () => {
        if (session.data) {
          await applyTheme(theme)
        }
      }),
    sizeWindow: () =>
      serially(async () => {
        if (session.data) {
          await sizeWindow()
        }
      }),
    capture: () =>
      serially(async () => {
        const before = session
        try {
          showData()
          await sizeWindow()
          const saved: Array<string> = []
          for (const theme of screenshotThemes) {
            await applyTheme(theme)
            await effects.waitUntilRendered()
            saved.push(await effects.captureWindow(theme))
          }
          return saved
        } finally {
          await restore(before)
        }
      }),
  }
}

function nextFrame(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => resolve())
    // A webview that isn't drawing has no frames to wait for.
    setTimeout(resolve, 100)
  })
}

/**
 * Resolves once the app has drawn what `client` holds: fonts loaded, no query
 * fetching for three frames running (a query can start only once the one
 * before it has rendered), then time for the window's material.
 */
async function waitUntilRendered(client: QueryClient | null): Promise<void> {
  await document.fonts.ready
  const deadline = Date.now() + 10_000
  let quietFrames = 0
  while (quietFrames < 3 && Date.now() < deadline) {
    await nextFrame()
    const busy = client !== null && (client.isFetching() > 0 || client.isMutating() > 0)
    quietFrames = busy ? 0 : quietFrames + 1
  }
  await new Promise((resolve) => setTimeout(resolve, materialSettleMs))
}

/**
 * Dev only: carries out the Developer menu's choices. Returns the query
 * client the app runs on while demo data is on: a fresh one each time, so
 * the app's own cache, and what is persisted of it, never holds demo data.
 */
export function useDemoMode(): QueryClient | null {
  const { mode, forceTheme } = useTheme()
  const toast = useToast()
  const [client, setClient] = useState<QueryClient | null>(null)
  // The demo's app state reports the mode the app is in, so the app doesn't
  // switch to, and store, another.
  const currentMode = useEffectEvent(() => mode)
  const reportFailure = useEffectEvent((error: unknown, title = 'Demo mode failed') => {
    toast.error(title, extractErrorMessage(error))
  })
  const reportSaved = useEffectEvent((saved: Array<string>) => {
    toast.success('Screenshots saved', saved.join(', '))
  })

  useEffect(() => {
    let demoClient: QueryClient | null = null
    const controller = createDemoController({
      showDemoData: (on) => {
        setDemoEnabled(on, { themeMode: currentMode() })
        demoClient = on ? createQueryClient() : null
        setClient(demoClient)
      },
      applyTheme: async (theme) => {
        forceTheme(theme)
        await devSetWindowTheme(theme)
      },
      sizeWindow: devPrepareScreenshotWindow,
      restoreWindow: devRestoreWindow,
      waitUntilRendered: () => waitUntilRendered(demoClient),
      captureWindow: devCaptureWindow,
    })
    const unlisteners = [
      listen<boolean>('dev-demo-data', (event) => controller.setData(event.payload).catch(reportFailure)),
      listen<DevTheme>('dev-theme', (event) => controller.setTheme(event.payload).catch(reportFailure)),
      listen('dev-screenshot-size', () => controller.sizeWindow().catch(reportFailure)),
      listen('dev-capture-screenshots', () =>
        controller.capture().then(reportSaved, (error) => reportFailure(error, 'Screenshots not saved')),
      ),
    ]
    return () => {
      for (const unlisten of unlisteners) {
        void unlisten.then((stop) => stop())
      }
    }
  }, [forceTheme])

  return client
}
