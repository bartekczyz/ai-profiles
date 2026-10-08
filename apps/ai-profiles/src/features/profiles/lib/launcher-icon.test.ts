import { describe, expect, it } from 'vitest'

import { launcherIconControls, pngDataUrl, resetIcon } from './launcher-icon'

const picked = {
  kind: 'custom',
  file: new File([new Uint8Array([0x89])], 'logo.png', { type: 'image/png' }),
  preview: 'data:image/png;base64,cGlja2Vk',
} as const
const own = 'data:image/png;base64,b3du'
const generated = 'data:image/png;base64,Z2VuZXJhdGVk'

describe('launcherIconControls', () => {
  it('shows the generated icon, with nothing to remove', () => {
    expect(launcherIconControls(null, null, generated)).toEqual({ icon: generated, canRemove: false })
  })

  it('shows an icon of the user’s, which can be removed', () => {
    expect(launcherIconControls(own, null, generated)).toEqual({ icon: own, canRemove: true })
  })

  it('shows a picked image at once, before it is saved', () => {
    for (const customIcon of [null, own]) {
      expect(launcherIconControls(customIcon, picked, generated)).toEqual({ icon: picked.preview, canRemove: true })
    }
  })

  it('shows the generated icon once the user’s is removed in the form', () => {
    expect(launcherIconControls(own, { kind: 'generated' }, generated)).toEqual({ icon: generated, canRemove: false })
  })

  it('has no icon to show while the generated one is still being rendered', () => {
    expect(launcherIconControls(null, null, null).icon).toBeNull()
  })

  it('shows nothing until the profile’s own icon is known, rather than one it may not have', () => {
    expect(launcherIconControls(undefined, null, generated)).toEqual({ icon: null, canRemove: false })
  })

  it('shows a picked image even before the profile’s own icon is known', () => {
    expect(launcherIconControls(undefined, picked, generated)).toEqual({ icon: picked.preview, canRemove: true })
  })
})

describe('resetIcon', () => {
  it('removes the icon a profile has', () => {
    expect(resetIcon(true)).toEqual({ kind: 'generated' })
  })

  it('only drops the picked image when the profile has no icon of its own', () => {
    expect(resetIcon(false)).toBeNull()
  })
})

describe('pngDataUrl', () => {
  it('encodes the bytes as a PNG data URL', () => {
    expect(pngDataUrl(new Uint8Array([0x89, 0x50, 0x4e, 0x47]).buffer)).toBe('data:image/png;base64,iVBORw==')
  })

  it('is null for no bytes, which is how the backend says there is no icon', () => {
    expect(pngDataUrl(new ArrayBuffer(0))).toBeNull()
  })

  it('encodes an image larger than one chunk', () => {
    const large = new Uint8Array(100_000).fill(65)
    expect(pngDataUrl(large.buffer)).toBe(`data:image/png;base64,${btoa('A'.repeat(100_000))}`)
  })
})
