import { focusManager } from '@tanstack/react-query'
import { getCurrentWindow } from '@tauri-apps/api/window'

/**
 * Drives TanStack Query's focusManager from real Tauri window focus.
 *
 * The webview's default visibility heuristics report "focused" almost
 * always, so interval refetches (usage polling) run 24/7 — including
 * overnight, where an unauthorized response on an unattended poll used to
 * mark profile credentials dead with nobody around. With focusManager
 * driven by the actual window state, refetchInterval pauses while the
 * window is unfocused (refetchIntervalInBackground defaults to false) and
 * refetchOnWindowFocus fetches fresh data the moment the user returns.
 *
 * Returns the Tauri unlisten function.
 */
export async function syncQueryFocusWithWindow(): Promise<() => void> {
  const appWindow = getCurrentWindow()
  focusManager.setFocused(await appWindow.isFocused())
  return appWindow.onFocusChanged((event) => {
    focusManager.setFocused(event.payload)
  })
}
