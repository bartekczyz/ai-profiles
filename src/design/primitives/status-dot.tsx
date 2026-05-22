import { cn } from '@/design/lib/cn'

type StatusTone = 'success' | 'warning' | 'danger' | 'neutral'

type StatusDotProps = {
  pulse?: boolean
  tone?: StatusTone
  className?: string
}

const toneClasses: Record<StatusTone, string> = {
  success: 'bg-green',
  warning: 'bg-amber',
  danger: 'bg-red',
  neutral: 'bg-muted',
}

const ringClasses: Record<StatusTone, string> = {
  success: 'shadow-[0_0_0_3px_color-mix(in_oklab,var(--color-green)_18%,transparent)]',
  warning: 'shadow-[0_0_0_3px_color-mix(in_oklab,var(--color-amber)_18%,transparent)]',
  danger: 'shadow-[0_0_0_3px_color-mix(in_oklab,var(--color-red)_18%,transparent)]',
  neutral: '',
}

export function StatusDot({ pulse = false, tone = 'neutral', className }: StatusDotProps) {
  return (
    <span
      aria-hidden
      data-tone={tone}
      className={cn('inline-block h-1.5 w-1.5 rounded-full', toneClasses[tone], pulse && ringClasses[tone], className)}
    />
  )
}

export type { StatusDotProps, StatusTone }
