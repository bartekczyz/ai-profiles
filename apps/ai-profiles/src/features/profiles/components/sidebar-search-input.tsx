import type { Ref } from 'react'

import { Search } from 'lucide-react'

import { ariaKeyshortcutsFor, Kbd } from '@/design'

type Props = {
  value?: string
  placeholder?: string
  inputRef?: Ref<HTMLInputElement>
  onChange?: (next: string) => void
}

/**
 * Local filter input for the sidebar profile list. Wired in `sidebar.tsx`
 * to the visible-profiles array — purely a name filter. The global ⌘F
 * shortcut focuses this input (registered in `app.tsx`); the ref is
 * threaded down so the parent can imperatively focus on hotkey.
 */
export function SidebarSearchInput({ value = '', placeholder = 'Search profiles…', inputRef, onChange }: Props) {
  return (
    <div className="relative mb-3.5 px-1">
      <Search
        aria-hidden
        className="pointer-events-none absolute top-1/2 left-3 h-3.5 w-3.5 -translate-y-1/2 text-muted-strong"
        strokeWidth={2}
      />
      <input
        ref={inputRef}
        type="search"
        aria-label="Search profiles"
        aria-keyshortcuts={ariaKeyshortcutsFor('focus-search')}
        value={value}
        onChange={(event) => onChange?.(event.target.value)}
        placeholder={placeholder}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        className="w-full appearance-none rounded-md border border-border bg-white/55 py-[6px] pr-9 pl-7 text-[12.5px] text-ink placeholder:text-muted-strong outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) focus:border-orange/55 focus:bg-white focus:shadow-[0_0_0_3px_color-mix(in_oklab,var(--color-orange)_12%,transparent)] dark:bg-white/4 dark:focus:bg-white/6"
      />
      <span className="pointer-events-none absolute right-2.5 top-0.75 ">
        <Kbd variant="subtle" shortcutId="focus-search" />
      </span>
    </div>
  )
}
