// SPDX-License-Identifier: MIT
import { useEffect, useState } from 'react'

import { relaunch } from '@tauri-apps/plugin-process'
import { check, type Update } from '@tauri-apps/plugin-updater'

type UpdaterStatus =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'up-to-date' }
  | { kind: 'available'; update: Update }
  | { kind: 'installing' }
  | { kind: 'error'; message: string }

const sixHoursMs = 6 * 60 * 60 * 1000

type StatusSetter = (next: UpdaterStatus) => void

async function runCheckInto(setStatus: StatusSetter): Promise<void> {
  setStatus({ kind: 'checking' })
  try {
    const update = await check()
    if (update === null) {
      setStatus({ kind: 'up-to-date' })
      return
    }
    setStatus({ kind: 'available', update })
  } catch (caught) {
    setStatus({
      kind: 'error',
      message: caught instanceof Error ? caught.message : String(caught),
    })
  }
}

/**
 * Polls Tauri's updater plugin on mount and again every six hours. Exposes
 * a discriminated `status` plus manual `check` / `installAndRestart`
 * triggers so the toast component and Settings → System can drive their
 * own UI from the same source of truth.
 *
 * Tauri 2's `downloadAndInstall` does NOT relaunch the app on its own —
 * the process plugin's `relaunch()` has to be called explicitly after,
 * otherwise the user sees "Installing…" forever and has to manually
 * quit + reopen to pick up the new binary.
 *
 * Implementation note: the polling body lives in `runCheckInto` (a
 * module-level function) so the effect dependency list stays empty — the
 * effect must run exactly once on mount and own the interval lifecycle,
 * regardless of how often React re-renders.
 */
export function useUpdater() {
  const [status, setStatus] = useState<UpdaterStatus>({ kind: 'idle' })

  async function manualCheck() {
    await runCheckInto(setStatus)
  }

  async function installAndRestart() {
    if (status.kind !== 'available') {
      return
    }
    const update = status.update
    setStatus({ kind: 'installing' })
    try {
      await update.downloadAndInstall()
      await relaunch()
    } catch (caught) {
      setStatus({
        kind: 'error',
        message: caught instanceof Error ? caught.message : String(caught),
      })
    }
  }

  useEffect(() => {
    void runCheckInto(setStatus)
    const interval = setInterval(() => {
      void runCheckInto(setStatus)
    }, sixHoursMs)
    return () => clearInterval(interval)
  }, [])

  return { status, check: manualCheck, installAndRestart }
}

export type { UpdaterStatus }
