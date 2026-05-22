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
  default:
    'text-muted bg-white/70 border border-border shadow-[inset_0_-1px_0_rgba(0,0,0,0.04)] dark:bg-white/5 dark:border-white/10',
  onOrange: 'text-white bg-white/15 border border-white/30 shadow-[inset_0_-1px_0_rgba(0,0,0,0.08)]',
  subtle: 'text-muted-strong bg-transparent border border-border/70 dark:border-white/10',
}

export function Kbd({ variant = 'default', shortcutId, className, children }: KbdProps) {
  const content = shortcutId ? formatShortcut(getShortcut(shortcutId).keys) : children
  return (
    <kbd
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
