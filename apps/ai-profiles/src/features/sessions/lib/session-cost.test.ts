import type { SessionCost } from '@/lib/types'

import { describe, expect, it } from 'vitest'

import { makeSession } from '../test/make-session'
import { costSum, shownUsd } from './session-cost'

/**
 * A cost of `usd`, covering as much of the use as `coverage` says.
 */
function makeCost(usd: number, coverage: SessionCost['coverage'] = 'complete'): SessionCost {
  return { totalUsd: usd, byModel: [{ model: 'Opus 5.5', usd }], unpriced: [], coverage }
}

describe('shownUsd', () => {
  it('shows the total unless none of the use could be priced', () => {
    expect(shownUsd(makeCost(4.5, 'partial'))).toBe(4.5)
    expect(shownUsd(makeCost(0, 'unpriced'))).toBeNull()
    expect(shownUsd(undefined)).toBeNull()
  })
})

describe('costSum', () => {
  const sessions = [makeSession({ id: 'a' }), makeSession({ id: 'b' }), makeSession({ id: 'c' })]

  it('adds up the rows shown, and says when it leaves use out', () => {
    expect(costSum(sessions, { a: makeCost(1.5), b: makeCost(3), hidden: makeCost(100) })).toEqual({
      usd: 4.5,
      partial: false,
    })
    expect(costSum(sessions, { a: makeCost(1.5), c: makeCost(0, 'unpriced') })).toEqual({ usd: 1.5, partial: true })
  })

  it('is nothing when no row has a cost', () => {
    expect(costSum(sessions, { hidden: makeCost(100) })).toBeNull()
  })
})
