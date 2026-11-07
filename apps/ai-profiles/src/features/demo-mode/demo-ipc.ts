import type { InvokeArgs, InvokeOptions, invoke } from '@tauri-apps/api/core'
import type { DemoSeed, DemoState } from './fixtures'

import { interceptCommands } from '@/lib/commands'

import { createDemoState, demoHandlers } from './fixtures'

/**
 * Where a command goes: answered from the fixtures, on to the backend, or
 * refused.
 */
type CommandRoute = 'fixture' | 'backend' | 'reject'

/**
 * Pure: where `command` goes. With demo data off, everything reaches the
 * backend. On, an app command is answered from the fixtures, and one without
 * a `handled` answer is refused rather than reaching the user's real data;
 * plugin commands (`plugin:…`) and the dev tools' own (`dev_…`) still reach
 * the backend.
 */
export function routeCommand(command: string, enabled: boolean, handled: (command: string) => boolean): CommandRoute {
  if (!enabled || command.startsWith('plugin:') || command.startsWith('dev_')) {
    return 'backend'
  }
  return handled(command) ? 'fixture' : 'reject'
}

type Invoke = typeof invoke

const handledCommands = new Set(Object.keys(demoHandlers))

let state: DemoState | null = null

/**
 * Wraps `backend` so that, while demo data is on, app commands are answered
 * from the fixtures instead.
 */
export function createDemoInvoke(backend: Invoke): Invoke {
  return function demoInvoke<T>(command: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T> {
    const route = routeCommand(command, state !== null, (name) => handledCommands.has(name))
    if (route === 'backend') {
      return backend<T>(command, args, options)
    }
    const current = state
    if (route === 'reject' || current === null) {
      return Promise.reject({ kind: 'Validation', message: `Demo data has no answer for ${command}.` })
    }
    // A microtask later, like the backend's answers, with a thrown error as
    // the rejection.
    return Promise.resolve().then(() => demoHandlers[command](current, args as never) as T)
  }
}

/**
 * Puts the demo between the app's commands and the backend. Dormant until
 * `setDemoEnabled(true)`.
 */
export function installDemoIpc(): void {
  interceptCommands(createDemoInvoke)
}

/**
 * Turns demo data on, as a fresh demo seeded with `seed`, or off.
 */
export function setDemoEnabled(enabled: boolean, seed: DemoSeed = { themeMode: 'system' }): void {
  state = enabled ? createDemoState(seed) : null
}
