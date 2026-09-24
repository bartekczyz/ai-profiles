import type { RepairReport } from '@/lib/types'

import { describe, expect, it } from 'vitest'

import { repairToast } from './repair-toast'

/**
 * A report of `repaired` sessions and the reasons others were skipped for.
 */
function report(
  repaired: number,
  reasons: Array<string> = [],
  memoryConflicts: Array<string> = [],
  warnings: Array<string> = [],
): RepairReport {
  return {
    repaired,
    skipped: reasons.map((reason, index) => ({ id: `s${index}`, reason })),
    memoryConflicts,
    warnings,
  }
}

describe('repairToast', () => {
  it('reports a repair that moved everything as a success', () => {
    expect(repairToast(report(3), 'Personal')).toEqual({
      tone: 'success',
      title: 'Sessions repaired',
      description: '3 repaired · 0 skipped',
    })
  })

  it('says why sessions were skipped, each reason once, and how many more there are', () => {
    const toast = repairToast(
      report(1, ['Close it in the terminal first', 'Close it in the terminal first', 'A', 'B', 'C']),
      'Personal',
    )
    expect(toast.tone).toBe('success')
    expect(toast.description).toBe('1 repaired · 5 skipped. Close it in the terminal first; A; B; +1 more')
  })

  it('reports a repair that moved nothing as a failure', () => {
    const toast = repairToast(report(0, ['Personal already has different files of it']), 'Personal')
    expect(toast.tone).toBe('error')
    expect(toast.description).toBe('0 repaired · 1 skipped. Personal already has different files of it')
  })

  it('names the memory the profile kept its own of', () => {
    expect(repairToast(report(2, [], ['deploy.md', 'style.md']), 'Personal').description).toBe(
      '2 repaired · 0 skipped. Personal kept its own memory of deploy.md, style.md',
    )
  })

  it('names what a repair left behind', () => {
    expect(
      repairToast(
        report(1, [], [], ['/Users/me/profile/a.jsonl is still also at /Users/me/.claude/a.jsonl']),
        'Personal',
      ).description,
    ).toBe('1 repaired · 0 skipped. /Users/me/profile/a.jsonl is still also at /Users/me/.claude/a.jsonl')
  })

  it('says so when nothing needed repair', () => {
    expect(repairToast(report(0), 'Personal').tone).toBe('info')
  })
})
