import { describe, expect, it } from 'vitest'

import { retryUnlessNotInstalled } from './retry'

describe('retryUnlessNotInstalled', () => {
  it('tries a failed query once more', () => {
    expect(retryUnlessNotInstalled(0, { kind: 'Io', message: 'boom' })).toBe(true)
    expect(retryUnlessNotInstalled(1, { kind: 'Io', message: 'boom' })).toBe(false)
    expect(retryUnlessNotInstalled(0, new Error('boom'))).toBe(true)
  })

  it('never tries again when the tool the query needs isn’t installed', () => {
    expect(retryUnlessNotInstalled(0, { kind: 'NotInstalled', message: 'Install the Codex CLI' })).toBe(false)
  })
})
