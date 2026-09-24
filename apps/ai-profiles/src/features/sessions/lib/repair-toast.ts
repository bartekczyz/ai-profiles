import type { RepairReport } from '@/lib/types'

/**
 * The toast that tells the user what a repair did.
 */
export type RepairToast = {
  /**
   * How it reads: a success when any session was repaired, an error when
   * sessions needed it and none was, else just information.
   */
  tone: 'success' | 'error' | 'info'
  /**
   * Its title.
   */
  title: string
  /**
   * The counts, the reasons sessions were skipped for, and the memory the
   * profile kept its own of.
   */
  description: string
}

/**
 * How many distinct skip reasons the toast names before it counts the rest.
 */
const shownReasons = 3

/**
 * What the toast after repairing `profileLabel`'s sessions says of `report`:
 * how many were repaired and skipped, each distinct reason for skipping once,
 * the first few of them with a count of the rest, and the memory files the
 * profile kept its own of.
 */
export function repairToast(report: RepairReport, profileLabel: string): RepairToast {
  const parts = [`${report.repaired} repaired · ${report.skipped.length} skipped`]
  const reasons = [...new Set(report.skipped.map((skipped) => skipped.reason))]
  if (reasons.length > 0) {
    const more = reasons.length - shownReasons
    parts.push([...reasons.slice(0, shownReasons), ...(more > 0 ? [`+${more} more`] : [])].join('; '))
  }
  if (report.memoryConflicts.length > 0) {
    parts.push(`${profileLabel} kept its own memory of ${report.memoryConflicts.join(', ')}`)
  }
  const description = parts.join('. ')
  if (report.repaired > 0) {
    return { tone: 'success', title: 'Sessions repaired', description }
  }
  if (report.skipped.length > 0) {
    return { tone: 'error', title: 'No sessions repaired', description }
  }
  return { tone: 'info', title: 'Nothing needed repair', description }
}
