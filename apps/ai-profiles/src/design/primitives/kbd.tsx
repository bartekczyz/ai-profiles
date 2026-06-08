import type { ReactNode } from 'react'

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
  onOrange: 'text-white bg-white/20 border border-white/40 shadow-[inset_0_-1px_0_rgba(0,0,0,0.08)]',
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
        'inline-flex h-[18px] min-w-[18px] items-center justify-center px-[5px] rounded-xs font-mono text-[10px] font-medium leading-none tracking-normal',
        variantClasses[variant],
        className,
      )}
    >
      {content}
    </kbd>
  )
}

type KbdGroupProps = {
  className?: string
  children: ReactNode
}

export function KbdGroup({ className, children }: KbdGroupProps) {
  return <span className={cn('inline-flex items-center gap-[2px]', className)}>{children}</span>
}

export type { KbdProps, KbdVariant }
