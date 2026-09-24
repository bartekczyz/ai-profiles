import { describe, expect, it } from 'vitest'

import { extractErrorKind, extractErrorMessage } from './extract-error-message'

describe('extractErrorMessage', () => {
  it('returns the message of a real Error', () => {
    expect(extractErrorMessage(new Error('boom'))).toBe('boom')
  })

  it('returns a string as-is', () => {
    expect(extractErrorMessage('plain string')).toBe('plain string')
  })

  it('reads the .message field on AppError objects from Rust', () => {
    expect(extractErrorMessage({ kind: 'Validation', message: 'name already in use' })).toBe('name already in use')
  })

  it('falls back when .message is missing or empty', () => {
    expect(extractErrorMessage({})).toBe('Something went wrong.')
    expect(extractErrorMessage({ message: '' })).toBe('Something went wrong.')
    expect(extractErrorMessage(undefined)).toBe('Something went wrong.')
    expect(extractErrorMessage(null)).toBe('Something went wrong.')
  })

  it('respects a custom fallback', () => {
    expect(extractErrorMessage({}, 'Could not save profile.')).toBe('Could not save profile.')
  })
})

describe('extractErrorKind', () => {
  it('reads the .kind field on AppError objects from Rust', () => {
    expect(extractErrorKind({ kind: 'NotInstalled', message: 'Install the Codex CLI' })).toBe('NotInstalled')
  })

  it('is null for anything without a kind', () => {
    expect(extractErrorKind(new Error('boom'))).toBeNull()
    expect(extractErrorKind('plain string')).toBeNull()
    expect(extractErrorKind({ kind: 7 })).toBeNull()
    expect(extractErrorKind(undefined)).toBeNull()
  })
})
