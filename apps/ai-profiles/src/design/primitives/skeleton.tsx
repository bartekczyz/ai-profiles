import type { HTMLAttributes } from 'react'

import { cn } from '@/design/lib/cn'

type SkeletonShape = 'text' | 'block' | 'circle'

type SkeletonProps = {
  shape?: SkeletonShape
} & HTMLAttributes<HTMLDivElement>

const shapeClasses: Record<SkeletonShape, string> = {
  text: 'h-3 rounded-full',
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
        'bg-black/[0.055] dark:bg-white/[0.06] animate-[design-skeleton_1.6s_ease-in-out_infinite]',
        shapeClasses[shape],
        className,
      )}
      {...rest}
    />
  )
}

export type { SkeletonProps, SkeletonShape }
