import type { ReactNode } from 'react'

import { createContext, useContext, useEffect, useState } from 'react'

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

function resolveSystem(): ResolvedTheme {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return 'light'
  }
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

function resolveMode(mode: ThemeMode): ResolvedTheme {
  if (mode === 'system') {
    return resolveSystem()
  }
  return mode
}

function applyResolved(resolved: ResolvedTheme) {
  if (typeof document === 'undefined') {
    return
  }
  document.documentElement.setAttribute('data-theme', resolved)
}

/**
 * Controls the active theme.
 *
 * - `mode` is the user's choice: 'light' | 'system' | 'dark'.
 * - When mode is 'system', the provider mirrors `prefers-color-scheme` to
 *   `[data-theme]`, so downstream CSS only needs to read that attribute.
 * - Pass `mode` to control externally (e.g., from app-state). When `mode`
 *   is omitted the provider keeps its own internal state, seeded by
 *   `defaultMode` (defaults to 'system').
 */
export function ThemeProvider({ children, mode, defaultMode = 'system', onModeChange }: ThemeProviderProps) {
  const isControlled = mode !== undefined
  const [internalMode, setInternalMode] = useState<ThemeMode>(defaultMode)
  const activeMode = isControlled ? mode : internalMode
  const [resolved, setResolved] = useState<ResolvedTheme>(() => resolveMode(activeMode))

  useEffect(() => {
    setResolved(resolveMode(activeMode))
  }, [activeMode])

  useEffect(() => {
    applyResolved(resolved)
  }, [resolved])

  useEffect(() => {
    if (activeMode !== 'system') {
      return
    }
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
      return
    }
    const media = window.matchMedia('(prefers-color-scheme: dark)')
    function handleChange() {
      setResolved(media.matches ? 'dark' : 'light')
    }
    media.addEventListener('change', handleChange)
    return () => {
      media.removeEventListener('change', handleChange)
    }
  }, [activeMode])

  function setMode(next: ThemeMode) {
    if (!isControlled) {
      setInternalMode(next)
    }
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
