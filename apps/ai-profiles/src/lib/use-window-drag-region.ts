import type { RefObject } from 'react'

import { useEffect } from 'react'

import { getCurrentWindow } from '@tauri-apps/api/window'

/**
 * Makes `ref`'s element drag the window, the way the title bar strip does.
 * Presses that land on a control inside it (a button, a link, a field, a menu
 * trigger) are left alone, so a toolbar can carry its own buttons and still
 * move the window from its empty space.
 */
export function useWindowDragRegion(ref: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const node = ref.current
    if (!node) {
      return
    }
    function handleMouseDown(event: MouseEvent) {
      if (event.buttons !== 1) {
        return
      }
      if (event.target instanceof Element && event.target.closest('button, a, input, [role="button"]')) {
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
  }, [ref])
}
