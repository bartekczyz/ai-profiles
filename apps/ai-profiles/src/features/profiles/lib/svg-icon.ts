/**
 * The side, in pixels, an SVG icon is drawn at: the largest size a launcher's
 * icon comes in, so the PNG it becomes passes the backend's checks as any
 * other picked image does.
 */
const rasterSide = 1024

/**
 * The largest SVG file accepted. Icons are a few kilobytes; anything near this
 * is embedded bitmaps or not an icon at all.
 */
const maxSvgBytes = 5 * 1024 * 1024

/**
 * The namespace an SVG document's root element is in.
 */
const svgNamespace = 'http://www.w3.org/2000/svg'

/**
 * Whether `file` is meant as an SVG: by its type, or by its name where the
 * system reports no type.
 */
export function isSvgFile(file: File): boolean {
  return file.type === 'image/svg+xml' || (file.type === '' && file.name.toLowerCase().endsWith('.svg'))
}

/**
 * Pure: `text` as an SVG document sized to be drawn at `side` pixels a side,
 * or what is wrong with it, in words the user can act on. It has to parse as
 * SVG, and say how wide and tall it is (a `viewBox`, or plain `width` and
 * `height`), with the two equal: an icon is square, as a picked PNG must be.
 */
export function prepareSvgIcon(text: string, side = rasterSide): { svg: string } | { error: string } {
  const parsed = new DOMParser().parseFromString(text, 'image/svg+xml')
  const root = parsed.documentElement
  if (parsed.getElementsByTagName('parsererror').length > 0 || root.namespaceURI !== svgNamespace) {
    return { error: 'That file isn’t a valid SVG image.' }
  }
  const size = intrinsicSize(root)
  if (size === null) {
    return { error: 'The SVG has no size. Give it a viewBox, or a width and height.' }
  }
  const [width, height] = size
  if (Math.abs(width - height) > Math.max(width, height) * 1e-3) {
    return { error: `The icon has to be square, and this SVG is ${width} × ${height}.` }
  }
  // Drawn at exactly this size, whatever it said: an SVG with only a viewBox
  // has no size of its own to draw at.
  root.setAttribute('width', String(side))
  root.setAttribute('height', String(side))
  return { svg: new XMLSerializer().serializeToString(root) }
}

/**
 * Width and height the root `<svg>` declares: its viewBox's, else its `width`
 * and `height` as plain numbers (or pixels). `null` when neither says.
 */
function intrinsicSize(root: Element): [number, number] | null {
  const viewBox = root
    .getAttribute('viewBox')
    ?.trim()
    .split(/[\s,]+/)
    .map(Number)
  if (viewBox?.length === 4 && viewBox.every(Number.isFinite)) {
    const [, , width, height] = viewBox
    return width > 0 && height > 0 ? [width, height] : null
  }
  const width = pixels(root.getAttribute('width'))
  const height = pixels(root.getAttribute('height'))
  return width !== null && height !== null ? [width, height] : null
}

/**
 * A length given as a plain number or in pixels, as a number; `null` for any
 * other unit, or none.
 */
function pixels(length: string | null): number | null {
  const match = length?.trim().match(/^(\d*\.?\d+)(px)?$/)
  const value = match ? Number(match[1]) : Number.NaN
  return value > 0 ? value : null
}

/**
 * The SVG in `file`, checked as {@link prepareSvgIcon} does and drawn into a
 * square PNG, for the launcher-icon pipeline to take like any picked PNG.
 * Rejects with what is wrong with it. Drawn through an `<img>`, so whatever
 * scripts or external resources the SVG names are never run or fetched.
 */
export async function svgToPng(file: File): Promise<File> {
  if (file.size > maxSvgBytes) {
    throw new Error(`The SVG is too large. Pick one of at most ${maxSvgBytes / (1024 * 1024)} MB.`)
  }
  const prepared = prepareSvgIcon(await file.text())
  if ('error' in prepared) {
    throw new Error(prepared.error)
  }
  const image = new Image()
  image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(prepared.svg)}`
  try {
    await image.decode()
  } catch {
    throw new Error('The SVG image can’t be drawn.')
  }
  const canvas = document.createElement('canvas')
  canvas.width = rasterSide
  canvas.height = rasterSide
  const context = canvas.getContext('2d')
  if (context === null) {
    throw new Error('The SVG image can’t be drawn.')
  }
  context.drawImage(image, 0, 0, rasterSide, rasterSide)
  // A canvas an SVG has tainted (one with a foreignObject, in WebKit) refuses
  // to be read back.
  const png = await new Promise<Blob | null>((resolve, reject) => {
    try {
      canvas.toBlob(resolve, 'image/png')
    } catch (caught) {
      reject(caught)
    }
  }).catch(() => null)
  if (png === null) {
    throw new Error('The SVG image can’t be drawn.')
  }
  return new File([png], file.name.replace(/\.svg$/i, '.png'), { type: 'image/png' })
}
