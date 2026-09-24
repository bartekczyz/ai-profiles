import { describe, expect, it } from 'vitest'

import { repairOfferShown } from './repair-offer'

describe('repairOfferShown', () => {
  it('hides the offer when nothing needs repair', () => {
    expect(repairOfferShown([], [])).toBe(false)
  })

  it('shows the offer when nothing was dismissed', () => {
    expect(repairOfferShown(['a'], [])).toBe(true)
  })

  it('keeps the offer away while only dismissed sessions need repair', () => {
    expect(repairOfferShown(['a', 'b'], ['a', 'b', 'c'])).toBe(false)
  })

  it('brings the offer back when a new session needs repair', () => {
    expect(repairOfferShown(['a', 'd'], ['a', 'b'])).toBe(true)
  })
})
