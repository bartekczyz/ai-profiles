import type { ReactNode } from 'react'

import { isValidElement } from 'react'

import { formatShortcut } from '@/design/keyboard/format-shortcut'
import { getShortcut, type ShortcutId } from '@/design/keyboard/shortcuts'
import { cn } from '@/design/lib/cn'

type KbdVariant = 'default' | 'onOrange' | 'subtle'

type KbdProps = {
  variant?: KbdVariant
  /**
   * Render the key for a registered shortcut. The chip then stays in
   * lockstep with the registry — renaming `Mod+K` → `Mod+Shift+K`
   * updates the chip everywhere automatically.
   *
   * When omitted, `children` is rendered as-is (ad-hoc chips).
   */
  shortcutId?: ShortcutId
  className?: string
  children?: ReactNode
}

const variantClasses: Record<KbdVariant, string> = {
  // The chip should *blend* with its surface — a key cap pressed into the
  // panel, not stamped on top. Faint background, faint border, muted text:
  // visible enough to read as a key cap but not so heavy that it
  // outweighs neighbouring icons or text. (Earlier attempts with a
  // stronger border made the chip optically taller than the 12px icons it
  // sits next to in the sidebar rows.)
  default:
    'text-muted bg-cream-2 border border-border/50 dark:text-ink-soft dark:bg-white/[0.06] dark:border-white/[0.08]',
  onOrange: 'text-current opacity-70 bg-current/10 border border-current/25',
  // Subtle is for inline hint strips — borderless on light, faint border
  // on dark, slightly stronger text than `text-muted` so it still reads.
  subtle: 'text-muted bg-transparent border border-border/70 dark:text-ink-soft dark:border-white/[0.12]',
}

export function Kbd({ variant = 'default', shortcutId, className, children }: KbdProps) {
  const content = shortcutId ? formatShortcut(getShortcut(shortcutId).keys) : children
  return (
    // Kbd chips are decorative — the accompanying button label or row name
    // already conveys the action. Hiding the chip from the a11y tree keeps
    // accessible names tight ("Delete" rather than "Delete ⌘ Backspace") and
    // lets buttons with trailingKbd be looked up by their visible label.
    <kbd
      aria-hidden
      className={cn(
        // `font-kbd`, not `font-mono` — see the token's note: Geist Mono has
        // no modifier glyphs, so a mono chip renders "⌘F" in two typefaces.
        'inline-flex h-[18px] min-w-[18px] items-center justify-center px-[5px] rounded-xs font-kbd text-[10px] font-medium leading-none tracking-normal',
        variantClasses[variant],
        className,
      )}
    >
      {content}
    </kbd>
  )
}

/**
 * The key a `<Kbd>` element shows, as plain text — for surfaces that state a
 * shortcut in words (a button's tooltip) rather than as a chip. `null` for
 * anything that isn't a `<Kbd>`.
 */
export function kbdText(node: ReactNode): string | null {
  if (!isValidElement<KbdProps>(node) || node.type !== Kbd) {
    return null
  }
  const { shortcutId, children } = node.props
  if (shortcutId) {
    return formatShortcut(getShortcut(shortcutId).keys)
  }
  return typeof children === 'string' ? children : null
}

type KbdGroupProps = {
  className?: string
  children: ReactNode
}

export function KbdGroup({ className, children }: KbdGroupProps) {
  return <span className={cn('inline-flex items-center gap-[2px]', className)}>{children}</span>
}

export type { KbdProps, KbdVariant }
