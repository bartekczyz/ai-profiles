import { describe, expect, it } from 'vitest'

import { pickDmg, type ReleaseAsset } from './release'

function asset(name: string): ReleaseAsset {
  return { name, browser_download_url: `https://example/${name}`, size: 1024 }
}

describe('pickDmg', () => {
  it('returns null for an empty assets list', () => {
    expect(pickDmg([])).toBeNull()
  })

  it('returns null when there are no dmg assets', () => {
    const result = pickDmg([asset('something.tar.gz'), asset('notes.txt')])
    expect(result).toBeNull()
  })

  it('prefers aarch64 over x64 when both are present', () => {
    const result = pickDmg([asset('ai-profiles-1.0.0-x86_64.dmg'), asset('ai-profiles-1.0.0-aarch64.dmg')])
    expect(result?.name).toBe('ai-profiles-1.0.0-aarch64.dmg')
  })

  it('also recognises arm64 in the name', () => {
    const result = pickDmg([asset('ai-profiles-1.0.0-x64.dmg'), asset('ai-profiles-1.0.0-arm64.dmg')])
    expect(result?.name).toBe('ai-profiles-1.0.0-arm64.dmg')
  })

  it('falls back to x64 when no arm build exists', () => {
    const result = pickDmg([asset('ai-profiles-1.0.0-x64.dmg')])
    expect(result?.name).toBe('ai-profiles-1.0.0-x64.dmg')
  })

  it('falls back to any dmg when arch is unknown', () => {
    const result = pickDmg([asset('ai-profiles-1.0.0-universal.dmg')])
    expect(result?.name).toBe('ai-profiles-1.0.0-universal.dmg')
  })

  it('ignores non-dmg assets when picking', () => {
    const result = pickDmg([asset('source.tar.gz'), asset('ai-profiles-1.0.0-aarch64.dmg')])
    expect(result?.name).toBe('ai-profiles-1.0.0-aarch64.dmg')
  })
})
