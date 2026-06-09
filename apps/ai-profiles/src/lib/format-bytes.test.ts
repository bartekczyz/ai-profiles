import { describe, expect, it } from 'vitest'

import { formatBytes } from './format-bytes'

describe('formatBytes', () => {
  it('collapses non-positive and non-finite inputs to 0 B', () => {
    expect(formatBytes(0)).toBe('0 B')
    expect(formatBytes(-1)).toBe('0 B')
    expect(formatBytes(Number.NaN)).toBe('0 B')
    expect(formatBytes(Number.POSITIVE_INFINITY)).toBe('0 B')
  })

  it('reports raw bytes under 1 KiB', () => {
    expect(formatBytes(1)).toBe('1 B')
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(1023)).toBe('1023 B')
  })

  it('shows one decimal under 10 of a unit', () => {
    expect(formatBytes(1024)).toBe('1.0 KB')
    expect(formatBytes(1536)).toBe('1.5 KB')
    expect(formatBytes(9 * 1024)).toBe('9.0 KB')
  })

  it('rounds to an integer at and above 10 of a unit', () => {
    expect(formatBytes(10 * 1024)).toBe('10 KB')
    expect(formatBytes(248 * 1024 * 1024)).toBe('248 MB')
    expect(formatBytes(1024 * 1024 * 1024 + 400 * 1024 * 1024)).toBe('1.4 GB')
  })

  it('caps the largest unit at TB', () => {
    expect(formatBytes(2 * 1024 ** 4)).toBe('2.0 TB')
    expect(formatBytes(2000 * 1024 ** 4)).toBe('2000 TB')
  })
})
