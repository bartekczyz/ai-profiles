import type { Profile } from '@/lib/types'

import { useState } from 'react'

import { Cog, Plus } from 'lucide-react'

import { Button, Kbd } from '@/design'

import { SidebarBrandMark } from './sidebar-brand-mark'
import { SidebarProfileRow } from './sidebar-profile-row'
import { SidebarSearchInput } from './sidebar-search-input'

type Props = {
  profiles: Array<Profile>
  selectedId: string | null
  onSelect: (id: string) => void
  onCreate: () => void
  onSettings: () => void
}

export function Sidebar({ profiles, selectedId, onSelect, onCreate, onSettings }: Props) {
  const [query, setQuery] = useState('')
  const filtered =
    query.trim().length === 0
      ? profiles
      : profiles.filter((profile) => profile.name.toLowerCase().includes(query.trim().toLowerCase()))

  return (
    <aside className="relative flex w-64 shrink-0 flex-col border-r border-border bg-cream-2 px-3 pt-11 pb-3">
      <SidebarBrandMark />
      <SidebarSearchInput value={query} onChange={setQuery} />
      <div className="px-2.5 pt-1.5 pb-2 font-mono text-[9.5px] font-medium uppercase tracking-[0.1em] text-muted-strong">
        Profiles
      </div>
      <ul className="flex flex-1 flex-col gap-px overflow-y-auto pr-0.5">
        {filtered.map((profile) => {
          // index is taken from the unfiltered list so kbd chips stay stable as the user types.
          const index = profiles.indexOf(profile)
          return (
            <li key={profile.id}>
              <SidebarProfileRow
                profile={profile}
                index={index}
                selected={profile.id === selectedId}
                onSelect={() => onSelect(profile.id)}
              />
            </li>
          )
        })}
      </ul>
      <footer className="mt-2 flex items-center gap-2 border-t border-border pt-2.5">
        <Button
          variant="primary"
          size="sm"
          className="flex-1 rounded-full"
          leadingIcon={<Plus className="h-3.5 w-3.5" strokeWidth={2.25} />}
          trailingKbd={<Kbd variant="onOrange">⌘N</Kbd>}
          onClick={onCreate}
        >
          New profile
        </Button>
        <button
          type="button"
          onClick={onSettings}
          aria-label="Open settings"
          title="Settings (⌘,)"
          className="grid h-7 w-[30px] cursor-pointer place-items-center rounded-sm border border-border bg-white/60 text-muted transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:bg-white hover:text-ink dark:bg-white/[0.04] dark:hover:bg-white/[0.08] dark:hover:text-ink"
        >
          <Cog className="h-3.5 w-3.5" strokeWidth={1.75} />
        </button>
      </footer>
    </aside>
  )
}
