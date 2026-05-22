import { Search } from 'lucide-react'

type Props = {
  value?: string
  placeholder?: string
  onChange?: (next: string) => void
}

/**
 * Local filter input for the sidebar profile list. Wired in `sidebar.tsx`
 * to the visible-profiles array — purely a name filter, no keyboard
 * shortcut of its own. (The global ⌘K opens the command palette, which is
 * a different — and more powerful — surface.)
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
        aria-label="Search profiles"
        value={value}
        onChange={(event) => onChange?.(event.target.value)}
        placeholder={placeholder}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        className="w-full appearance-none rounded-md border border-border bg-white/55 py-[5px] pr-2 pl-7 text-[12.5px] text-ink placeholder:text-muted-strong outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) focus:border-orange/55 focus:bg-white focus:shadow-[0_0_0_3px_color-mix(in_oklab,var(--color-orange)_12%,transparent)] dark:bg-white/4 dark:focus:bg-white/6"
      />
    </div>
  )
}
