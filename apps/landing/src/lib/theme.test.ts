import { describe, expect, it } from 'vitest'

import { themeScript } from './theme'

describe('themeScript', () => {
  it('is a non-empty string', () => {
    expect(themeScript.length).toBeGreaterThan(0)
  })

  it('reads localStorage before prefers-color-scheme', () => {
    expect(themeScript.indexOf('localStorage.getItem')).toBeLessThan(themeScript.indexOf('matchMedia'))
  })

  it('falls back to light on error', () => {
    expect(themeScript).toContain("setAttribute('data-theme', 'light')")
  })
})
