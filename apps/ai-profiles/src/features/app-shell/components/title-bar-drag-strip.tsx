import { useEffect, useRef } from 'react'

import { getCurrentWindow } from '@tauri-apps/api/window'

/**
 * Tauri overlay-style title bar dragger.
 *
 * `titleBarStyle: Overlay` in tauri.conf.json drops the system title bar so
 * the traffic-light area becomes empty content space. Without an explicit
 * drag handler the user can't move the window (the traffic lights themselves
 * are buttons, not draggers). This 28px-tall transparent strip explicitly
 * calls Tauri's `startDragging()` on mousedown — the docs' recommended
 * approach for SPA-rendered drag regions, more reliable than relying on the
 * attribute-based auto-binding which often misses React-rendered nodes.
 * Double-click toggles maximize, matching native macOS behaviour.
 * - z-10 keeps it below dialogs (z-40/50) so overlay surfaces stay
 *   interactive.
 * - It sits over the sidebar's `pt-11` empty area and the right pane's
 *   `pt-10`/`py-9` top padding, so no interactive content is obscured.
 */
export function TitleBarDragStrip() {
  // Native (not React-synthetic) mousedown listener. React's synthetic events
  // fire AFTER the browser has finished delivering the native event, by which
  // point macOS has already decided what to do with the click — when the
  // window is active that means `startDragging()` becomes a no-op. Binding the
  // listener directly to the element via `addEventListener` (the pattern
  // Tauri's own docs use) intercepts the event early enough that drag works
  // in both active and inactive window states.
  const dragStripRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const node = dragStripRef.current
    if (!node) {
      return
    }
    function handleMouseDown(event: MouseEvent) {
      if (event.buttons !== 1) {
        return
      }
      const appWindow = getCurrentWindow()
      if (event.detail === 2) {
        void appWindow.toggleMaximize()
      } else {
        void appWindow.startDragging()
      }
    }
    node.addEventListener('mousedown', handleMouseDown)
    return () => node.removeEventListener('mousedown', handleMouseDown)
  }, [])

  return <div ref={dragStripRef} aria-hidden className="absolute inset-x-0 top-0 z-10 h-7" />
}
