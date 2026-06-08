import { describe, expect, it } from 'vitest'

import { isValidHexColor, presetColors } from './colors'

describe('isValidHexColor', () => {
  it('accepts uppercase #RRGGBB', () => {
    expect(isValidHexColor('#7C3AED')).toBe(true)
  })

  it('accepts lowercase #rrggbb', () => {
    expect(isValidHexColor('#abcdef')).toBe(true)
  })

  it('rejects missing hash', () => {
    expect(isValidHexColor('7C3AED')).toBe(false)
  })

  it('rejects short form', () => {
    expect(isValidHexColor('#7C3')).toBe(false)
  })

  it('rejects non-hex chars', () => {
    expect(isValidHexColor('#7C3AEZ')).toBe(false)
  })

  it('rejects the empty string', () => {
    expect(isValidHexColor('')).toBe(false)
  })

  it('all presetColors are themselves valid', () => {
    for (const preset of presetColors) {
      expect(isValidHexColor(preset)).toBe(true)
    }
  })
})
