import type { HTMLAttributes, ReactNode } from 'react'

import { cn } from '@/design/lib/cn'

type CardProps = HTMLAttributes<HTMLDivElement>

export function Card({ className, children, ...rest }: CardProps) {
  return (
    <div
      className={cn(
        'rounded-xl border border-border bg-white dark:bg-cream-2 transition-[border-color,box-shadow] duration-(--duration-base) ease-(--ease-natural) hover:border-border-strong hover:shadow-card-hover',
        className,
      )}
      {...rest}
    >
      {children}
    </div>
  )
}

type CardRowProps = {
  leading?: ReactNode
  label: ReactNode
  detail?: ReactNode
  trailing?: ReactNode
  className?: string
}

export function CardRow({ leading, label, detail, trailing, className }: CardRowProps) {
  return (
    <div className={cn('flex items-center gap-3 px-4 py-3 border-b border-border-soft last:border-b-0', className)}>
      {leading ? <span className="flex-shrink-0">{leading}</span> : null}
      <div className="flex-1 min-w-0">
        <div className="text-body text-ink tracking-[-0.005em]">{label}</div>
        {detail ? <div className="font-mono text-mono text-muted-strong mt-0.5">{detail}</div> : null}
      </div>
      {trailing ? <div className="flex-shrink-0">{trailing}</div> : null}
    </div>
  )
}

export type { CardProps, CardRowProps }
