import type { ProfileAccount } from '@/lib/types'

import { describe, expect, it } from 'vitest'

import { accountLabel, accountTitle } from './account-line'

function account(overrides: Partial<ProfileAccount> = {}): ProfileAccount {
  return { email: 'ada@example.com', name: 'Ada', organization: 'Ada Ltd', plan: 'Max', ...overrides }
}

describe('accountLabel', () => {
  it('says nothing while the account is still being read', () => {
    expect(accountLabel(undefined)).toBeNull()
  })

  it('says so when nothing on disk names an account', () => {
    expect(accountLabel(null)).toBe('Not signed in')
  })

  it('leads with the email and carries the plan', () => {
    expect(accountLabel(account())).toBe('ada@example.com · Max')
    expect(accountLabel(account({ plan: null }))).toBe('ada@example.com')
  })

  it('falls back to the name, then to the plan alone', () => {
    expect(accountLabel(account({ email: null }))).toBe('Ada · Max')
    expect(accountLabel(account({ email: null, name: null }))).toBe('Max')
    expect(accountLabel(account({ email: null, name: null, plan: null }))).toBeNull()
  })
})

describe('accountTitle', () => {
  it('spells the account out, skipping what is missing', () => {
    expect(accountTitle(account())).toBe('Ada · ada@example.com · Ada Ltd · Max')
    expect(accountTitle(account({ name: null, organization: null }))).toBe('ada@example.com · Max')
    expect(accountTitle(null)).toBeUndefined()
    expect(accountTitle(undefined)).toBeUndefined()
  })
})
