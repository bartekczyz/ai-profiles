import { describe, expect, it } from 'vitest'

import { narrowSpend, narrowWindow } from './narrow-usage'

describe('narrowSpend', () => {
  it('keeps a well-formed spend block', () => {
    const result = narrowSpend({ usedMinor: 7788, limitMinor: 30000, currency: 'GBP', exponent: 2, percent: 26 })
    expect(result).toEqual({ usedMinor: 7788, limitMinor: 30000, currency: 'GBP', exponent: 2, percent: 26 })
  })

  it('drops non-record input', () => {
    expect(narrowSpend('garbage')).toBeUndefined()
    expect(narrowSpend(null)).toBeUndefined()
  })

  it('drops a missing or empty currency', () => {
    expect(narrowSpend({ usedMinor: 100, currency: undefined })).toBeUndefined()
    expect(narrowSpend({ usedMinor: 100, currency: '' })).toBeUndefined()
  })

  it('drops a fractional or NaN usedMinor', () => {
    expect(narrowSpend({ usedMinor: 77.5, currency: 'GBP' })).toBeUndefined()
    expect(narrowSpend({ usedMinor: Number.NaN, currency: 'GBP' })).toBeUndefined()
  })

  it('drops a negative usedMinor', () => {
    expect(narrowSpend({ usedMinor: -1, currency: 'GBP' })).toBeUndefined()
  })

  it('falls back limitMinor to null when missing, zero, or fractional', () => {
    expect(narrowSpend({ usedMinor: 100, currency: 'GBP', limitMinor: undefined })?.limitMinor).toBeNull()
    expect(narrowSpend({ usedMinor: 100, currency: 'GBP', limitMinor: 0 })?.limitMinor).toBeNull()
    expect(narrowSpend({ usedMinor: 100, currency: 'GBP', limitMinor: 1.5 })?.limitMinor).toBeNull()
  })

  it('defaults exponent to 2 when missing or out of the 0..6 range', () => {
    expect(narrowSpend({ usedMinor: 100, currency: 'GBP' })?.exponent).toBe(2)
    expect(narrowSpend({ usedMinor: 100, currency: 'GBP', exponent: 99 })?.exponent).toBe(2)
    expect(narrowSpend({ usedMinor: 100, currency: 'GBP', exponent: -1 })?.exponent).toBe(2)
  })

  it('keeps an exponent at the edges of the 0..6 range', () => {
    expect(narrowSpend({ usedMinor: 100, currency: 'GBP', exponent: 0 })?.exponent).toBe(0)
    expect(narrowSpend({ usedMinor: 100, currency: 'GBP', exponent: 6 })?.exponent).toBe(6)
  })

  it('falls back percent to null when missing, negative, or non-finite', () => {
    expect(narrowSpend({ usedMinor: 100, currency: 'GBP', percent: undefined })?.percent).toBeNull()
    expect(narrowSpend({ usedMinor: 100, currency: 'GBP', percent: -1 })?.percent).toBeNull()
    expect(narrowSpend({ usedMinor: 100, currency: 'GBP', percent: Number.NaN })?.percent).toBeNull()
  })

  it('keeps a zero percent', () => {
    expect(narrowSpend({ usedMinor: 100, currency: 'GBP', percent: 0 })?.percent).toBe(0)
  })
})

describe('narrowWindow', () => {
  it('drops non-record input', () => {
    expect(narrowWindow('garbage')).toBeNull()
    expect(narrowWindow(null)).toBeNull()
  })

  it('preserves utilization above 100 (over-limit users)', () => {
    expect(narrowWindow({ utilization: 105, resetsAt: null })?.utilization).toBe(105)
  })

  it('coerces a NaN or negative utilization to null', () => {
    expect(narrowWindow({ utilization: Number.NaN, resetsAt: null })?.utilization).toBeNull()
    expect(narrowWindow({ utilization: -1, resetsAt: null })?.utilization).toBeNull()
  })

  it('keeps a zero utilization', () => {
    expect(narrowWindow({ utilization: 0, resetsAt: null })?.utilization).toBe(0)
  })

  it('falls back resetsAt to null when not a string', () => {
    expect(narrowWindow({ utilization: 1, resetsAt: undefined })?.resetsAt).toBeNull()
    expect(narrowWindow({ utilization: 1, resetsAt: 12345 })?.resetsAt).toBeNull()
  })

  it('keeps a valid windowDurationMins', () => {
    expect(narrowWindow({ utilization: 1, resetsAt: null, windowDurationMins: 300 })).toHaveProperty(
      'windowDurationMins',
      300,
    )
  })

  it('omits windowDurationMins when missing, zero, or fractional', () => {
    expect(narrowWindow({ utilization: 1, resetsAt: null })).not.toHaveProperty('windowDurationMins')
    expect(narrowWindow({ utilization: 1, resetsAt: null, windowDurationMins: 0 })).not.toHaveProperty(
      'windowDurationMins',
    )
    expect(narrowWindow({ utilization: 1, resetsAt: null, windowDurationMins: 1.5 })).not.toHaveProperty(
      'windowDurationMins',
    )
  })

  it('trims a well-formed label', () => {
    expect(narrowWindow({ utilization: 1, resetsAt: null, label: '  Fable  ' })).toHaveProperty('label', 'Fable')
  })

  it('drops a blank or non-string label', () => {
    expect(narrowWindow({ utilization: 1, resetsAt: null, label: '   ' })).not.toHaveProperty('label')
    expect(narrowWindow({ utilization: 1, resetsAt: null, label: 7 })).not.toHaveProperty('label')
    expect(narrowWindow({ utilization: 1, resetsAt: null, label: undefined })).not.toHaveProperty('label')
  })
})
