import type { ReactNode } from 'react'

import { createContext, useContext, useEffect, useState, useSyncExternalStore } from 'react'

export type ThemeMode = 'light' | 'system' | 'dark'
export type ResolvedTheme = 'light' | 'dark'

type ThemeContextValue = {
  mode: ThemeMode
  resolved: ResolvedTheme
  setMode: (mode: ThemeMode) => void
}

const ThemeContext = createContext<ThemeContextValue | null>(null)

type ThemeProviderProps = {
  children: ReactNode
  mode?: ThemeMode
  defaultMode?: ThemeMode
  onModeChange?: (mode: ThemeMode) => void
}

// prefers-color-scheme is an external store; useSyncExternalStore is the
// supported way to subscribe to it (no effect-managed state, no tear).
function subscribePrefersColorScheme(callback: () => void): () => void {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return () => {}
  }
  const media = window.matchMedia('(prefers-color-scheme: dark)')
  media.addEventListener('change', callback)
  return () => media.removeEventListener('change', callback)
}

function readPrefersColorScheme(): ResolvedTheme {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return 'light'
  }
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

function getServerSnapshot(): ResolvedTheme {
  return 'light'
}

const STORAGE_KEY = 'claude-profiles:theme'

// Mirror the user's mode to localStorage so the inline script in index.html
// can read it synchronously on next launch — that's what eliminates the
// brief flash of the default theme before React mounts. This is the
// next-themes pattern: a tiny synchronous bootstrap + a single React effect
// for ongoing changes.
function writeStoredMode(mode: ThemeMode) {
  if (typeof window === 'undefined') {
    return
  }
  try {
    window.localStorage.setItem(STORAGE_KEY, mode)
  } catch {
    // private mode / quota — nothing to do; first paint next launch will fall back to system
  }
}

/**
 * Controls the active theme.
 *
 * - `mode` is the user's choice: 'light' | 'system' | 'dark'.
 * - When mode is 'system', the resolved theme tracks `prefers-color-scheme`
 *   via useSyncExternalStore — no effect plumbing, no first-render tear.
 * - The provider writes `[data-theme]` on the html element so downstream
 *   CSS only needs to read that attribute.
 * - Pass `mode` to control externally (e.g., from app-state). When `mode`
 *   is omitted the provider keeps its own internal state, seeded by
 *   `defaultMode` (defaults to 'system').
 */
export function ThemeProvider({ children, mode, defaultMode = 'system', onModeChange }: ThemeProviderProps) {
  const isControlled = mode !== undefined
  const [internalMode, setInternalMode] = useState<ThemeMode>(defaultMode)
  const activeMode = isControlled ? mode : internalMode

  const systemResolved = useSyncExternalStore(subscribePrefersColorScheme, readPrefersColorScheme, getServerSnapshot)
  const resolved: ResolvedTheme = activeMode === 'system' ? systemResolved : activeMode

  useEffect(() => {
    if (typeof document === 'undefined') {
      return
    }
    document.documentElement.setAttribute('data-theme', resolved)
  }, [resolved])

  function setMode(next: ThemeMode) {
    if (!isControlled) {
      setInternalMode(next)
    }
    writeStoredMode(next)
    if (onModeChange) {
      onModeChange(next)
    }
  }

  return <ThemeContext.Provider value={{ mode: activeMode, resolved, setMode }}>{children}</ThemeContext.Provider>
}

export function useTheme(): ThemeContextValue {
  const context = useContext(ThemeContext)
  if (!context) {
    throw new Error('useTheme must be used inside a <ThemeProvider>')
  }
  return context
}
