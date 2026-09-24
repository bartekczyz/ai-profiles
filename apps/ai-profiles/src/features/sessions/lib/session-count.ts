/**
 * `count` sessions, in words: `1 session`, `3 sessions`.
 */
export function sessionCount(count: number): string {
  return count === 1 ? '1 session' : `${count} sessions`
}
