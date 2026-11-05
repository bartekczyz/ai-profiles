import { describe, expect, it } from 'vitest'

import { formatShare, formatUsd } from './format-cost'

describe('formatting', () => {
  it('drops the cents from $100 up', () => {
    expect(formatUsd(4.5)).toBe('$4.50')
    expect(formatUsd(20)).toBe('$20')
    expect(formatUsd(1234.56)).toBe('$1,235')
  })

  it('shows an amount too small for cents as under a cent, not as $0.00', () => {
    expect(formatUsd(0.004)).toBe('<$0.01')
    expect(formatUsd(0.006)).toBe('$0.01')
    expect(formatUsd(0)).toBe('$0')
  })

  it('shows a share too small to round up as under 1%', () => {
    expect(formatShare(0.0001)).toBe('<1%')
    expect(formatShare(0)).toBe('0%')
    expect(formatShare(0.426)).toBe('43%')
  })
})
