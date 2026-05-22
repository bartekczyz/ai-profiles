import { cn } from '@/design/lib/cn'

type PulseTone = 'success' | 'warning' | 'danger'

type StatusPulseProps = {
  tone?: PulseTone
  className?: string
}

const toneClasses: Record<PulseTone, string> = {
  success: 'bg-green',
  warning: 'bg-amber',
  danger: 'bg-red',
}

/**
 * Continuously-pulsing status dot used on surface cards.
 * Animation pauses cleanly under prefers-reduced-motion (design/index.css).
 */
export function StatusPulse({ tone = 'success', className }: StatusPulseProps) {
  return (
    <span
      aria-hidden
      data-tone={tone}
      className={cn(
        'inline-block h-1.5 w-1.5 rounded-full shadow-pulse-success animate-[design-pulse_2.4s_ease-in-out_infinite]',
        toneClasses[tone],
        className,
      )}
    />
  )
}

export type { PulseTone, StatusPulseProps }
