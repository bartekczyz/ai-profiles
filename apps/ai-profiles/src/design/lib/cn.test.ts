import { describe, expect, it } from 'vitest'

import { cn } from './cn'

describe('cn', () => {
  it('joins multiple class names', () => {
    expect(cn('text-sm', 'font-medium')).toBe('text-sm font-medium')
  })

  it('dedupes conflicting tailwind classes (last one wins)', () => {
    expect(cn('text-sm', 'text-lg')).toBe('text-lg')
  })

  it('drops falsy values', () => {
    expect(cn('text-sm', false && 'hidden', null, undefined)).toBe('text-sm')
  })
})
