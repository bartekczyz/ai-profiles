import type { HTMLAttributes } from 'react'

import { cn } from '@/design/lib/cn'

type SkeletonShape = 'text' | 'block' | 'circle'

type SkeletonProps = {
  shape?: SkeletonShape
} & HTMLAttributes<HTMLDivElement>

const shapeClasses: Record<SkeletonShape, string> = {
  text: 'h-3 rounded-xs',
  block: 'rounded-md',
  circle: 'rounded-full',
}

/**
 * Token-driven placeholder. Animates a gentle shimmer between cream-2 and
 * cream-3; respects prefers-reduced-motion via the global CSS rule in
 * src/design/index.css.
 */
export function Skeleton({ shape = 'block', className, ...rest }: SkeletonProps) {
  return (
    <div
      aria-hidden
      data-shape={shape}
      className={cn(
        'bg-cream-2 dark:bg-cream-3 animate-[design-skeleton_1.4s_ease-in-out_infinite]',
        shapeClasses[shape],
        className,
      )}
      {...rest}
    />
  )
}

export type { SkeletonProps, SkeletonShape }
