import type { QueryClient } from '@tanstack/react-query'
import type { DevTheme } from '@/lib/types'

import { useEffect, useEffectEvent, useState } from 'react'

import { listen } from '@tauri-apps/api/event'

import { useTheme, useToast } from '@/design'
import { devPrepareScreenshotWindow, devRestoreWindow, devSetWindowTheme } from '@/lib/commands'
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
}

/**
 * The demo's state machine, over `effects`: theme and size only apply while
 * demo data is on, and turning it off puts the theme, then the window, then
 * the real data back.
 */
export function createDemoController(effects: DemoEffects): DemoController {
  let session = realApp
  let queue: Promise<void> = Promise.resolve()

  function serially(step: () => Promise<void>): Promise<void> {
    const run = queue.then(step)
    queue = run.catch(() => undefined)
    return run
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
        if (!on) {
          await restore(realApp)
          return
        }
        if (!session.data) {
          effects.showDemoData(true)
          session = { ...session, data: true }
        }
      }),
    setTheme: (theme) =>
      serially(async () => {
        if (!session.data) {
          return
        }
        // Noted first, so a half-done change is still undone.
        session = { ...session, theme }
        await effects.applyTheme(theme)
      }),
    sizeWindow: () =>
      serially(async () => {
        if (!session.data) {
          return
        }
        session = { ...session, sized: true }
        await effects.sizeWindow()
      }),
  }
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
  const reportFailure = useEffectEvent((error: unknown) => {
    toast.error('Demo mode failed', extractErrorMessage(error))
  })

  useEffect(() => {
    const controller = createDemoController({
      showDemoData: (on) => {
        setDemoEnabled(on, { themeMode: currentMode() })
        setClient(on ? createQueryClient() : null)
      },
      applyTheme: async (theme) => {
        forceTheme(theme)
        await devSetWindowTheme(theme)
      },
      sizeWindow: devPrepareScreenshotWindow,
      restoreWindow: devRestoreWindow,
    })
    const unlisteners = [
      listen<boolean>('dev-demo-data', (event) => controller.setData(event.payload).catch(reportFailure)),
      listen<DevTheme>('dev-theme', (event) => controller.setTheme(event.payload).catch(reportFailure)),
      listen('dev-screenshot-size', () => controller.sizeWindow().catch(reportFailure)),
    ]
    return () => {
      for (const unlisten of unlisteners) {
        void unlisten.then((stop) => stop())
      }
    }
  }, [forceTheme])

  return client
}
