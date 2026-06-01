import type { ReactNode } from 'react'
import type { AppId } from '@/lib/app-registry'

type GlyphSpec = {
  tint: string
  mark: ReactNode
}

/**
 * Per-app marks: deliberately not initials (both apps start with "C") and not
 * vendor logos. Claude = filled circle (solid identity); Codex = terminal
 * chevron › (prompt mark). Each glyph is tinted by its OWN app's accent —
 * independent of the window-wide `--app-accent`, which tracks the *selected*
 * app, whereas a glyph marks the app of the row it sits on.
 */
const glyphSpecs: Record<AppId, GlyphSpec> = {
  claude: {
    tint: 'var(--color-orange)',
    mark: <circle cx="4" cy="4" r="3.5" fill="white" />,
  },
  codex: {
    tint: 'var(--color-codex)',
    mark: (
      <polyline
        points="1,1.5 5,4 1,6.5"
        stroke="white"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    ),
  },
}

type Props = {
  app: AppId
  size?: number
}

/**
 * Small neutral mark distinguishing which app a sidebar row belongs to.
 * Shown only when the sidebar contains entries from more than one app-kind.
 */
export function AppGlyph({ app, size = 14 }: Props) {
  const { tint, mark } = glyphSpecs[app]
  const markSize = Math.round(size * 0.55)

  return (
    <span
      aria-hidden
      data-app-glyph={app}
      className="inline-grid shrink-0 place-items-center rounded-[3px]"
      style={{ width: size, height: size, background: tint }}
    >
      {/* biome-ignore lint/a11y/noSvgWithoutTitle: decorative — parent span is aria-hidden */}
      <svg
        aria-hidden
        width={markSize}
        height={markSize}
        viewBox="0 0 8 8"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
      >
        {mark}
      </svg>
    </span>
  )
}
