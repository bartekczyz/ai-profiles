import type { Profile } from '@/lib/types'

import { Cog } from 'lucide-react'

import { Button } from '@/design/ui/button'

import { ProfileList } from './profile-list'

type Props = {
  profiles: Array<Profile>
  selectedId: string | null
  onSelect: (id: string) => void
  onCreate: () => void
  onSettings: () => void
}

export function Sidebar({ profiles, selectedId, onSelect, onCreate, onSettings }: Props) {
  return (
    <aside className="flex w-[240px] shrink-0 flex-col border-r border-(--color-border) bg-transparent">
      <header className="px-4 py-4">
        <h1 className="text-sm font-semibold tracking-tight">claude-profiles</h1>
      </header>
      <ProfileList profiles={profiles} selectedId={selectedId} onSelect={onSelect} />
      <footer className="flex items-center gap-2 border-t border-(--color-border) p-3">
        <Button size="sm" className="flex-1" onClick={onCreate}>
          + New profile
        </Button>
        <Button size="sm" variant="ghost" onClick={onSettings} aria-label="Open settings">
          <Cog className="h-4 w-4" />
        </Button>
      </footer>
    </aside>
  )
}
