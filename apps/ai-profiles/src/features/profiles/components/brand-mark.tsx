import type { AppId } from '@/lib/app-registry'

import { cn } from '@/design'

type MarkProps = {
  size?: number
  className?: string
}

/**
 * Real vendor brand marks (Claude sunburst, OpenAI logo), rendered in their
 * native brand identity rather than tinted — these are logos, not glyphs.
 * Claude's mark is a fixed brand colour and keeps it across light/dark
 * themes; OpenAI's official mark is monochrome (black, white on dark
 * backgrounds per their own brand guidelines), so it renders via
 * `currentColor` and defaults to the `ink` text token so it still adapts.
 * Paths copied verbatim from the vendor-supplied `icon-claude.svg` asset and
 * from Simple Icons' CC0 `openai.svg` (simpleicons.org — no distinct
 * "ChatGPT" mark is published; OpenAI's mark is what its own apps use).
 */

function ClaudeMark({ size = 16, className }: MarkProps) {
  return (
    // biome-ignore lint/a11y/noSvgWithoutTitle: decorative brand mark (aria-hidden); always labelled by adjacent text
    <svg
      aria-hidden
      width={size}
      height={size}
      viewBox="0 0 256 256"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
    >
      <path
        d="M121.344 256L114.944 251.136L111.36 243.2L114.944 227.328L119.04 206.848L122.368 190.464L125.44 170.24L127.232 163.584L126.976 163.072L125.696 163.328L110.336 184.32L87.04 215.808L68.608 235.264L64.256 237.056L56.576 233.216L57.344 226.048L61.696 219.904L87.04 187.392L102.4 167.168L112.384 155.648L112.128 154.112H111.616L44.032 198.144L32 199.68L26.624 194.816L27.392 186.88L29.952 184.32L50.176 170.24L100.608 142.08L101.376 139.52L100.608 138.24H98.048L89.6 137.728L60.928 136.96L36.096 135.936L11.776 134.656L5.632 133.376L0 125.696L0.512 121.856L5.632 118.528L13.056 119.04L29.184 120.32L53.504 121.856L71.168 122.88L97.28 125.696H101.376L101.888 123.904L100.608 122.88L99.584 121.856L74.24 104.96L47.104 87.04L32.768 76.544L25.088 71.168L21.248 66.304L19.712 55.552L26.624 47.872L36.096 48.64L38.4 49.152L47.872 56.576L68.096 72.192L94.72 91.904L98.56 94.976L100.352 93.952V93.184L98.56 90.368L84.224 64.256L68.864 37.632L61.952 26.624L60.16 19.968C59.4773 17.664 59.136 15.104 59.136 12.288L67.072 1.53601L71.424 0L82.176 1.53601L86.528 5.37601L93.184 20.48L103.68 44.288L120.32 76.544L125.184 86.272L127.744 94.976L128.768 97.792H130.56V96.256L131.84 77.824L134.4 55.552L136.96 26.88L137.728 18.688L141.824 8.96001L149.76 3.84001L155.904 6.65601L161.024 14.08L160.256 18.688L157.44 38.4L151.296 69.376L147.456 90.368H149.76L152.32 87.552L162.816 73.728L180.48 51.712L188.16 43.008L197.376 33.28L203.264 28.672H214.272L222.208 40.704L218.624 53.248L207.36 67.584L197.888 79.616L184.32 97.792L176.128 112.384L176.896 113.408H178.688L209.152 106.752L225.792 103.936L245.248 100.608L254.208 104.704L255.232 108.8L251.648 117.504L230.656 122.624L206.08 127.488L169.472 136.192L168.96 136.448L169.472 137.216L185.856 138.752L193.024 139.264H210.432L242.688 141.568L251.136 147.2L256 153.856L255.232 159.232L242.176 165.632L224.768 161.536L183.808 151.808L169.984 148.48H167.936V149.504L179.712 161.024L200.96 180.224L227.84 205.056L229.12 211.2L225.792 216.32L222.208 215.808L198.656 197.888L189.44 189.952L168.96 172.8H167.68V174.592L172.288 181.504L197.376 219.136L198.656 230.656L196.864 234.24L190.208 236.544L183.296 235.264L168.448 214.784L153.344 191.488L141.056 170.752L139.776 171.776L132.352 249.088L129.024 252.928L121.344 256Z"
        fill="#D97757"
      />
    </svg>
  )
}

/**
 * OpenAI's mark — used for ChatGPT profiles since the desktop app they launch
 * is now ChatGPT, and OpenAI doesn't publish a distinct "ChatGPT" glyph
 * separate from their company mark.
 */
function ChatGptMark({ size = 16, className }: MarkProps) {
  return (
    // biome-ignore lint/a11y/noSvgWithoutTitle: decorative brand mark (aria-hidden); always labelled by adjacent text
    <svg
      aria-hidden
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={cn('text-ink', className)}
    >
      <path
        d="M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z"
        fill="currentColor"
      />
    </svg>
  )
}

/** Brand mark for an app, in its native identity. */
export function BrandMark({ app, size, className }: { app: AppId; size?: number; className?: string }) {
  if (app === 'codex') {
    return <ChatGptMark size={size} className={className} />
  }
  return <ClaudeMark size={size} className={className} />
}
