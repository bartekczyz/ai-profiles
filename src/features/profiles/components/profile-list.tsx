import type { Profile } from '@/lib/types'

type Props = {
  profiles: Array<Profile>
  selectedId: string | null
  onSelect: (id: string) => void
}

export function ProfileList({ profiles, selectedId, onSelect }: Props) {
  return (
    <ul className="flex-1 space-y-0.5 overflow-y-auto px-2 py-2">
      {profiles.map((profile) => {
        const isSelected = profile.id === selectedId
        return (
          <li key={profile.id}>
            <button
              type="button"
              onClick={() => onSelect(profile.id)}
              className={
                isSelected
                  ? 'flex w-full items-center gap-3 rounded-md bg-(--color-border) px-2 py-2'
                  : 'flex w-full items-center gap-3 rounded-md px-2 py-2 hover:bg-(--color-border)/50'
              }
            >
              <span className="inline-block h-3 w-3 shrink-0 rounded-full" style={{ background: profile.color }} />
              <span className="flex-1 truncate text-left text-sm">{profile.name}</span>
              <span className="flex shrink-0 gap-1 text-[10px] text-muted-foreground">
                {profile.surfaces.gui ? <span title="Desktop">GUI</span> : null}
                {profile.surfaces.cli ? <span title="CLI">CLI</span> : null}
              </span>
            </button>
          </li>
        )
      })}
    </ul>
  )
}
