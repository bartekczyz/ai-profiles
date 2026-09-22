import type { DefaultEntry } from '@/lib/types'

import { describe, expect, it } from 'vitest'

import { planDefaultProfileEdit, resetDefaultProfileEdit } from './default-profile-edit'

function entry(overrides: Partial<DefaultEntry> = {}): DefaultEntry {
  return {
    id: 'default:claude',
    app: 'claude',
    name: 'Claude',
    customName: null,
    color: null,
    surfaces: { gui: true, cli: true },
    ...overrides,
  }
}

describe('planDefaultProfileEdit', () => {
  it('has nothing to save while nothing changed', () => {
    expect(planDefaultProfileEdit(entry({ customName: 'Personal' }), ' Personal ', '')).toEqual({
      edit: {},
      valid: false,
    })
  })

  it('takes only the fields that changed, tidied', () => {
    expect(planDefaultProfileEdit(entry({ customName: 'Personal' }), 'Personal', ' #6A9BCC ')).toEqual({
      edit: { color: '#6a9bcc' },
      valid: true,
    })
    expect(planDefaultProfileEdit(entry({ color: '#6a9bcc' }), ' Work ', '#6a9bcc')).toEqual({
      edit: { name: 'Work' },
      valid: true,
    })
  })

  it('clears a colour, and refuses one that is not a colour', () => {
    expect(planDefaultProfileEdit(entry({ color: '#6a9bcc' }), '', '')).toEqual({ edit: { color: '' }, valid: true })
    expect(planDefaultProfileEdit(entry(), '', '#6a9b').valid).toBe(false)
  })
})

describe('resetDefaultProfileEdit', () => {
  it('clears only what was customised', () => {
    expect(resetDefaultProfileEdit(entry())).toEqual({})
    expect(resetDefaultProfileEdit(entry({ customName: 'Personal' }))).toEqual({ name: '' })
    expect(resetDefaultProfileEdit(entry({ customName: 'Personal', color: '#6a9bcc' }))).toEqual({
      name: '',
      color: '',
    })
  })
})
