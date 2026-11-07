import { useEffect } from 'react'

import { act, render } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { ThemeProvider, useTheme } from './theme-provider'

type MediaListener = (event: { matches: boolean }) => void

class MockMediaQueryList {
  matches: boolean
  private listeners: Set<MediaListener> = new Set()
  constructor(initial: boolean) {
    this.matches = initial
  }
  addEventListener(_event: string, listener: MediaListener) {
    this.listeners.add(listener)
  }
  removeEventListener(_event: string, listener: MediaListener) {
    this.listeners.delete(listener)
  }
  fire(matches: boolean) {
    this.matches = matches
    for (const listener of this.listeners) {
      listener({ matches })
    }
  }
}

let mockMedia: MockMediaQueryList

function ResolvedProbe() {
  const { resolved, mode } = useTheme()
  return <div data-testid="probe" data-resolved={resolved} data-mode={mode} />
}

function ForcedTheme({ theme }: { theme: 'light' | 'dark' | null }) {
  const { forceTheme } = useTheme()
  useEffect(() => {
    forceTheme(theme)
  }, [forceTheme, theme])
  return null
}

beforeEach(() => {
  mockMedia = new MockMediaQueryList(false)
  vi.stubGlobal('matchMedia', () => mockMedia as unknown as MediaQueryList)
  document.documentElement.removeAttribute('data-theme')
})

afterEach(() => {
  vi.unstubAllGlobals()
  document.documentElement.removeAttribute('data-theme')
  window.localStorage.clear()
})

describe('ThemeProvider', () => {
  it('applies the explicit dark mode to <html data-theme>', () => {
    render(
      <ThemeProvider defaultMode="dark">
        <ResolvedProbe />
      </ThemeProvider>,
    )
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
  })

  it('resolves system mode against prefers-color-scheme', () => {
    mockMedia.matches = true
    const { getByTestId } = render(
      <ThemeProvider defaultMode="system">
        <ResolvedProbe />
      </ThemeProvider>,
    )
    expect(getByTestId('probe').dataset.resolved).toBe('dark')
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
  })

  it('re-resolves when the system theme flips while mode is system', () => {
    const { getByTestId } = render(
      <ThemeProvider defaultMode="system">
        <ResolvedProbe />
      </ThemeProvider>,
    )
    expect(getByTestId('probe').dataset.resolved).toBe('light')
    act(() => {
      mockMedia.fire(true)
    })
    expect(getByTestId('probe').dataset.resolved).toBe('dark')
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
  })

  it('respects controlled mode prop', () => {
    const { rerender, getByTestId } = render(
      <ThemeProvider mode="light">
        <ResolvedProbe />
      </ThemeProvider>,
    )
    expect(getByTestId('probe').dataset.resolved).toBe('light')
    rerender(
      <ThemeProvider mode="dark">
        <ResolvedProbe />
      </ThemeProvider>,
    )
    expect(getByTestId('probe').dataset.resolved).toBe('dark')
  })

  it('shows a forced theme over the mode without storing it, until the force is lifted', () => {
    const { getByTestId, rerender } = render(
      <ThemeProvider defaultMode="light">
        <ResolvedProbe />
        <ForcedTheme theme="dark" />
      </ThemeProvider>,
    )
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
    expect(getByTestId('probe').dataset.mode).toBe('light')
    expect(window.localStorage.getItem('ai-profiles:theme')).toBeNull()

    rerender(
      <ThemeProvider defaultMode="light">
        <ResolvedProbe />
        <ForcedTheme theme={null} />
      </ThemeProvider>,
    )
    expect(document.documentElement.getAttribute('data-theme')).toBe('light')
  })
})
