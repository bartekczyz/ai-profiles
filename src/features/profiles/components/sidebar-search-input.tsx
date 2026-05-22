import { Search } from 'lucide-react'

import { Kbd } from '@/design'

type Props = {
  value?: string
  placeholder?: string
  onChange?: (next: string) => void
}

/**
 * Visual-only for now. Filtering is owned by the command palette (Phase 10);
 * this input is wired into the keyboard registry in Phase 11.
 */
export function SidebarSearchInput({ value = '', placeholder = 'Search profiles…', onChange }: Props) {
  return (
    <div className="relative mb-3.5 px-1">
      <Search
        aria-hidden
        className="pointer-events-none absolute top-1/2 left-3 h-3.5 w-3.5 -translate-y-1/2 text-muted-strong"
        strokeWidth={2}
      />
      <input
        type="search"
        value={value}
        onChange={(event) => onChange?.(event.target.value)}
        placeholder={placeholder}
        className="w-full appearance-none rounded-md border border-border bg-white/55 py-1.5 pr-12 pl-7 text-[12.5px] text-ink placeholder:text-muted-strong outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) focus:border-orange/55 focus:bg-white focus:shadow-[0_0_0_3px_color-mix(in_oklab,var(--color-orange)_12%,transparent)] dark:bg-white/4 dark:focus:bg-white/6"
      />
      <span className="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2">
        <Kbd>⌘K</Kbd>
      </span>
    </div>
  )
}
