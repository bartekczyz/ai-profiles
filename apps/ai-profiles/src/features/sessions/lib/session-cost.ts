import type { Session, SessionCost } from '@/lib/types'

/**
 * What a set of rows' costs add up to.
 */
export type CostSum = {
  /**
   * The sum of the rows' priced use, in USD.
   */
  usd: number
  /**
   * Some row's use is left out of it, priced in part or not at all.
   */
  partial: boolean
}

/**
 * The figure a row shows for `cost`, in USD: its total, unless none of its use
 * could be priced. `null` when there is none to show.
 */
export function shownUsd(cost: SessionCost | undefined): number | null {
  if (cost === undefined || cost.coverage === 'unpriced') {
    return null
  }
  return cost.totalUsd
}

/**
 * What `sessions` add up to by `costs`: `null` when none of them has a cost.
 */
export function costSum(sessions: Array<Session>, costs: Record<string, SessionCost>): CostSum | null {
  const own = sessions.map((session) => costs[session.id]).filter((cost) => cost !== undefined)
  if (own.length === 0) {
    return null
  }
  return {
    usd: own.reduce((sum, cost) => sum + (shownUsd(cost) ?? 0), 0),
    partial: own.some((cost) => cost.coverage !== 'complete'),
  }
}
