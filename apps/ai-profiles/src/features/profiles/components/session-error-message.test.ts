import { describe, expect, it } from 'vitest'

import { sessionErrorMessage } from './session-error-message'

describe('sessionErrorMessage', () => {
  it('drops the kind the backend leads its message with', () => {
    expect(sessionErrorMessage({ kind: 'Validation', message: 'validation error: Quit Claude (Default) first.' })).toBe(
      'Quit Claude (Default) first.',
    )
    expect(sessionErrorMessage({ kind: 'NotFound', message: 'not found: session x not found in P' })).toBe(
      'session x not found in P',
    )
  })

  it('leaves other messages alone', () => {
    expect(sessionErrorMessage(new Error('disk full'))).toBe('disk full')
    expect(sessionErrorMessage(undefined, 'Fallback.')).toBe('Fallback.')
  })
})
