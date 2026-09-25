import type { SidebarEntry, ThemeMode } from '@/lib/types'

import { useEffect } from 'react'

import { listen } from '@tauri-apps/api/event'

import { useTheme } from '@/design'
import { appFromEntry } from '@/features/profiles/api/use-sidebar-entries'

/**
 * Applies the theme mode persisted in AppState to the design system's theme.
 */
function useThemeSync(persistedThemeMode: ThemeMode) {
  const theme = useTheme()

  useEffect(() => {
    if (persistedThemeMode !== theme.mode) {
      theme.setMode(persistedThemeMode)
    }
  }, [persistedThemeMode, theme])
}

/**
 * Suppresses the system context menu app-wide in production builds — this is
 * a Tauri window, not a browser. Inputs and `[data-selectable=true]`
 * regions opt back in so a user can still right-click to paste into the
 * profile name field, etc.
 *
 * In dev (`pnpm tauri dev` / `vite dev`) we leave the context menu alone so
 * the webview's "Inspect Element" stays accessible while iterating.
 */
function useContextMenuSuppression() {
  useEffect(() => {
    if (import.meta.env.DEV) {
      return
    }
    function handleContextMenu(event: MouseEvent) {
      const target = event.target as HTMLElement | null
      if (!target) {
        event.preventDefault()
        return
      }
      if (target.closest('input, textarea, [contenteditable="true"], [data-selectable="true"]')) {
        return
      }
      event.preventDefault()
    }
    window.addEventListener('contextmenu', handleContextMenu)
    return () => window.removeEventListener('contextmenu', handleContextMenu)
  }, [])
}

/**
 * Bridges the macOS App menu's "About ai-profiles" item to our custom
 * dialog. The menu item (set up in src-tauri/src/lib.rs) emits the
 * `open-about` event; this listener catches it and calls `onOpen`.
 * Replaces the tiny native About panel macOS would otherwise show.
 */
function useAboutMenuEvent(onOpen: () => void) {
  useEffect(() => {
    const unlistenPromise = listen('open-about', () => {
      onOpen()
    })
    return () => {
      void unlistenPromise.then((unlisten) => {
        unlisten()
      })
    }
  }, [onOpen])
}

/**
 * Tints the whole window by the selected entry's app. When nothing is
 * selected (or this unmounts), clears the attribute so :root falls back to
 * --color-orange.
 */
function useAppTint(selected: SidebarEntry | null) {
  useEffect(() => {
    const app = selected ? appFromEntry(selected) : null
    if (app) {
      document.documentElement.dataset.app = app
    } else {
      delete document.documentElement.dataset.app
    }
    return () => {
      delete document.documentElement.dataset.app
    }
  }, [selected])
}

type WindowIntegrationInput = {
  /**
   * The theme mode persisted in AppState.
   */
  themeMode: ThemeMode
  /**
   * The selected sidebar entry, whose app tints the window.
   */
  selected: SidebarEntry | null
  /**
   * Called when the App menu's "About ai-profiles" item is chosen.
   */
  onOpenAbout: () => void
}

/**
 * Syncs the window with the app shell: persisted theme, context-menu
 * suppression, the App menu's About item and the per-app tint.
 */
export function useWindowIntegration({ themeMode, selected, onOpenAbout }: WindowIntegrationInput) {
  useThemeSync(themeMode)
  useContextMenuSuppression()
  useAboutMenuEvent(onOpenAbout)
  useAppTint(selected)
}
