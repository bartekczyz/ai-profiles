import type { HTMLAttributes } from 'react'

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
