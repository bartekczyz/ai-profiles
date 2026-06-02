import type * as React from 'react'

import { cn } from '@/design/lib/cn'

function Input({ className, type, ...props }: React.ComponentProps<'input'>) {
  return (
    <input
      type={type}
      data-slot="input"
      // Canonical field look — single source of truth shared by the text
      // fields and the Select trigger so they're visually identical.
      className={cn(
        'w-full appearance-none rounded-md border border-border bg-white px-3 py-2.5 font-sans text-[13.5px] text-ink outline-none transition-[border-color,box-shadow] duration-(--duration-snap) ease-(--ease-natural) placeholder:text-muted-strong focus:border-orange focus:shadow-[0_0_0_3px_color-mix(in_oklab,var(--color-orange)_15%,transparent)] disabled:cursor-not-allowed disabled:opacity-50 dark:bg-cream-2',
        className,
      )}
      {...props}
    />
  )
}

export { Input }
