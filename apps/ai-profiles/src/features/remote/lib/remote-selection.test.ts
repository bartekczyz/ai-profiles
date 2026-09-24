import { describe, expect, it } from 'vitest'

import { parseRemoteSelection, remoteSelectionId } from './remote-selection'

describe('remote selection ids', () => {
  it('round-trips a host and account', () => {
    const id = remoteSelectionId('7d1c-uuid', 'marcus2')
    expect(id).toBe('remote:7d1c-uuid:marcus2')
    expect(parseRemoteSelection(id)).toEqual({ hostId: '7d1c-uuid', account: 'marcus2' })
  })

  it('ignores local ids and malformed ones', () => {
    expect(parseRemoteSelection(null)).toBeNull()
    expect(parseRemoteSelection('default:claude')).toBeNull()
    expect(parseRemoteSelection('3f2a-profile-uuid')).toBeNull()
    expect(parseRemoteSelection('remote:')).toBeNull()
    expect(parseRemoteSelection('remote:host')).toBeNull()
    expect(parseRemoteSelection('remote:host:')).toBeNull()
    expect(parseRemoteSelection('remote::acct')).toBeNull()
  })
})
