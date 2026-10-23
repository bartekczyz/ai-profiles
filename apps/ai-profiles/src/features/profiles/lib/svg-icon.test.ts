import { describe, expect, it } from 'vitest'

import { isSvgFile, prepareSvgIcon } from './svg-icon'

const svg = (attributes: string) =>
  `<svg xmlns="http://www.w3.org/2000/svg" ${attributes}><rect width="1" height="1"/></svg>`

function rootSize(result: ReturnType<typeof prepareSvgIcon>) {
  if ('error' in result) {
    throw new Error(result.error)
  }
  const root = new DOMParser().parseFromString(result.svg, 'image/svg+xml').documentElement
  return [root.getAttribute('width'), root.getAttribute('height')]
}

describe('prepareSvgIcon', () => {
  it('sizes a square SVG to be drawn at the given side', () => {
    for (const attributes of [
      'viewBox="0 0 24 24"',
      'viewBox="0,0,24,24" width="10" height="99"',
      'width="64" height="64px"',
    ]) {
      expect(rootSize(prepareSvgIcon(svg(attributes), 1024))).toEqual(['1024', '1024'])
    }
  })

  it('refuses an SVG that is not square, with its size', () => {
    expect(prepareSvgIcon(svg('viewBox="0 0 32 24"'))).toEqual({
      error: 'The icon has to be square, and this SVG is 32 × 24.',
    })
  })

  it('refuses an SVG that does not say how large it is', () => {
    for (const attributes of ['', 'width="100%" height="100%"', 'viewBox="0 0 0 0"', 'width="2em" height="2em"']) {
      expect(prepareSvgIcon(svg(attributes))).toEqual({
        error: 'The SVG has no size. Give it a viewBox, or a width and height.',
      })
    }
  })

  it('refuses what is not SVG', () => {
    for (const text of ['<svg xmlns="http://www.w3.org/2000/svg"', '<html/>', '\u0089PNG\r\n', '']) {
      expect(prepareSvgIcon(text)).toEqual({ error: 'That file isn’t a valid SVG image.' })
    }
  })
})

describe('isSvgFile', () => {
  it('tells an SVG by its type, or by its name when it has none', () => {
    expect(isSvgFile(new File([], 'logo', { type: 'image/svg+xml' }))).toBe(true)
    expect(isSvgFile(new File([], 'Logo.SVG'))).toBe(true)
    expect(isSvgFile(new File([], 'logo.png', { type: 'image/png' }))).toBe(false)
    expect(isSvgFile(new File([], 'logo.svg', { type: 'image/png' }))).toBe(false)
  })
})
