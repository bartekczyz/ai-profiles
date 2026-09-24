import { describe, expect, it } from 'vitest'

import { sessionCount } from './session-count'

describe('sessionCount', () => {
  it('counts one session in the singular and any other number in the plural', () => {
    expect(sessionCount(1)).toBe('1 session')
    expect(sessionCount(68)).toBe('68 sessions')
    expect(sessionCount(0)).toBe('0 sessions')
  })
})
