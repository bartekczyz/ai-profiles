import type { ButtonHTMLAttributes, ReactNode, Ref } from 'react'

import { cn } from '@/design/lib/cn'

import { kbdText } from './kbd'

type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger'
type ButtonSize = 'sm' | 'md'

type ButtonProps = {
  variant?: ButtonVariant
  size?: ButtonSize
  leadingIcon?: ReactNode
  trailingIcon?: ReactNode
  /**
   * The button's keyboard shortcut, as a `<Kbd>`. Stated in the native
   * tooltip ("Close (⎋)") rather than drawn as a chip: Mac apps show
   * shortcuts in menus and help tags, not on the buttons themselves.
   */
  trailingKbd?: ReactNode
  ref?: Ref<HTMLButtonElement>
  children?: ReactNode
} & ButtonHTMLAttributes<HTMLButtonElement>

const baseClasses =
  'inline-flex shrink-0 items-center justify-center gap-1.5 whitespace-nowrap font-medium leading-none select-none outline-none transition-[background-color,border-color,color,filter,box-shadow,transform] duration-(--duration-snap) ease-(--ease-natural) focus-visible:ring-2 focus-visible:ring-orange/40 focus-visible:ring-offset-1 focus-visible:ring-offset-cream disabled:cursor-not-allowed disabled:opacity-60'

const variantClasses: Record<ButtonVariant, string> = {
  primary:
    'text-cream border-0 bg-ink shadow-[0_1px_2px_rgba(0,0,0,0.12),inset_0_1px_0_rgba(255,255,255,0.08)] hover:not-disabled:opacity-90 active:not-disabled:translate-y-px',
  secondary:
    'text-ink-soft bg-cream border border-border hover:not-disabled:bg-white hover:not-disabled:border-border-strong hover:not-disabled:text-ink dark:hover:not-disabled:bg-cream-3 shadow-[0_1px_0_rgba(40,30,20,0.03)]',
  ghost: 'text-muted bg-transparent border-0 hover:not-disabled:bg-cream-2 hover:not-disabled:text-ink',
  danger:
    'text-red bg-cream border border-border hover:not-disabled:border-red/45 hover:not-disabled:bg-white dark:hover:not-disabled:bg-cream-3',
}

const sizeClasses: Record<ButtonSize, string> = {
  sm: 'h-7 px-2.5 text-[12px] rounded-md',
  md: 'h-8 px-3 text-[12.5px] rounded-md',
}

export function Button({
  className,
  variant = 'secondary',
  size = 'md',
  leadingIcon,
  trailingIcon,
  trailingKbd,
  ref,
  children,
  type,
  title,
  ...rest
}: ButtonProps) {
  const shortcut = kbdText(trailingKbd)
  const hint = shortcut && typeof children === 'string' ? `${children} (${shortcut})` : (shortcut ?? undefined)
  return (
    <button
      ref={ref}
      type={type ?? 'button'}
      data-variant={variant}
      data-size={size}
      title={title ?? hint}
      className={cn(baseClasses, variantClasses[variant], sizeClasses[size], className)}
      {...rest}
    >
      {leadingIcon ? (
        <span
          aria-hidden
          className="inline-flex h-[var(--size-icon-sm)] w-[var(--size-icon-sm)] items-center justify-center"
        >
          {leadingIcon}
        </span>
      ) : null}
      {children !== undefined ? <span>{children}</span> : null}
      {trailingIcon ? (
        <span
          aria-hidden
          className="inline-flex h-[var(--size-icon-sm)] w-[var(--size-icon-sm)] items-center justify-center"
        >
          {trailingIcon}
        </span>
      ) : null}
    </button>
  )
}

export type { ButtonProps, ButtonSize, ButtonVariant }
