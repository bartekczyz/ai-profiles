import { describe, expect, it } from 'vitest'

import { shortenHomePath } from './shorten-home-path'

describe('shortenHomePath', () => {
  it('replaces the home directory with a tilde', () => {
    expect(shortenHomePath('/Users/ada/.local/bin/claude-work')).toBe('~/.local/bin/claude-work')
  })

  it('leaves paths outside the home directory untouched', () => {
    expect(shortenHomePath('/Applications/Claude (work).app')).toBe('/Applications/Claude (work).app')
  })

  it('shortens the home directory itself to a bare tilde', () => {
    expect(shortenHomePath('/Users/ada')).toBe('~')
  })

  it('only rewrites a home prefix at the start of the path', () => {
    expect(shortenHomePath('/var/tmp/Users/ada/cache')).toBe('/var/tmp/Users/ada/cache')
  })

  it('shortens against a given home, for paths from another machine', () => {
    expect(shortenHomePath('/home/marcus/code/api', '/home/marcus')).toBe('~/code/api')
    expect(shortenHomePath('/home/marcus', '/home/marcus/')).toBe('~')
    expect(shortenHomePath('/home/marcusb/x', '/home/marcus')).toBe('/home/marcusb/x')
    expect(shortenHomePath('/Users/ada/code', '/home/marcus')).toBe('/Users/ada/code')
  })
})
