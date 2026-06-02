import type { AppId } from '@/lib/app-registry'

import { BrandMark } from './brand-mark'

type Props = {
  app: AppId
  size?: number
}

/**
 * Marks which app a sidebar row belongs to, using the real vendor brand logo
 * (Claude / Codex) in its native colours. Shown when the sidebar contains
 * entries from more than one app-kind. The `data-app-glyph` attribute is
 * relied on by the sidebar tests to detect glyph presence.
 */
export function AppGlyph({ app, size = 14 }: Props) {
  return (
    <span
      aria-hidden
      data-app-glyph={app}
      className="inline-grid shrink-0 place-items-center"
      style={{ width: size, height: size }}
    >
      <BrandMark app={app} size={size} />
    </span>
  )
}
