import type { AccountStatus, ProfileAccount } from '@/lib/types'

import { describe, expect, it } from 'vitest'

import { accountLabel, accountTitle } from './account-line'

function signedIn(overrides: Partial<ProfileAccount> = {}): AccountStatus {
  return {
    status: 'signedIn',
    account: { email: 'ada@example.com', name: 'Ada', organization: 'Ada Ltd', plan: 'Max', ...overrides },
  }
}

describe('accountLabel', () => {
  it('says nothing while the account is still being read', () => {
    expect(accountLabel(undefined)).toBeNull()
  })

  it('says so when nothing is signed in', () => {
    expect(accountLabel({ status: 'signedOut' })).toBe('Not signed in')
  })

  it("says nothing when who is signed in can't be told", () => {
    expect(accountLabel({ status: 'unknown' })).toBeNull()
  })

  it('leads with the email and carries the plan', () => {
    expect(accountLabel(signedIn())).toBe('ada@example.com · Max')
    expect(accountLabel(signedIn({ plan: null }))).toBe('ada@example.com')
  })

  it('falls back to the name, then to the plan alone', () => {
    expect(accountLabel(signedIn({ email: null }))).toBe('Ada · Max')
    expect(accountLabel(signedIn({ email: null, name: null }))).toBe('Max')
    expect(accountLabel(signedIn({ email: null, name: null, plan: null }))).toBeNull()
  })
})

describe('accountTitle', () => {
  it('spells the account out, skipping what is missing', () => {
    expect(accountTitle(signedIn())).toBe('Ada · ada@example.com · Ada Ltd · Max')
    expect(accountTitle(signedIn({ name: null, organization: null }))).toBe('ada@example.com · Max')
  })

  it('has nothing to spell out for anything but a named account', () => {
    expect(accountTitle({ status: 'signedOut' })).toBeUndefined()
    expect(accountTitle({ status: 'unknown' })).toBeUndefined()
    expect(accountTitle(undefined)).toBeUndefined()
  })
})
