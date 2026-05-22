import type { Profile } from '@/lib/types'

import { formatDistanceToNow } from 'date-fns'

import { ariaKeyshortcutsFor, Button, Kbd } from '@/design'

type Props = {
  profile: Profile
  onEdit: () => void
}

/**
 * Header block: 44×44 inset-glossed swatch, profile name in display
 * weight, mono slug + last-used line, and a ghost Edit button (⌘E).
 */
export function ProfileDetailHeader({ profile, onEdit }: Props) {
  return (
    <header className="mb-5 flex items-start gap-4 border-b border-border-soft pb-5">
      <ProfileSwatch color={profile.color} />
      <div className="min-w-0 flex-1 pt-px">
        <h2 className="m-0 mb-1 text-display font-bold tracking-[-0.03em] text-ink leading-[1.05]">{profile.name}</h2>
        <p className="font-mono text-[12px] tracking-[-0.005em] text-muted">
          <span>{profile.slug}</span>
          <span className="mx-2 text-border">·</span>
          <span className="text-muted-strong">{formatLastUsed(profile.lastUsedAt)}</span>
        </p>
      </div>
      <div className="flex items-center gap-1 pt-1">
        <Button
          variant="ghost"
          size="sm"
          trailingKbd={<Kbd shortcutId="edit-selected" />}
          aria-keyshortcuts={ariaKeyshortcutsFor('edit-selected')}
          onClick={onEdit}
        >
          Edit
        </Button>
      </div>
    </header>
  )
}

function formatLastUsed(timestamp: string | null): string {
  if (!timestamp) {
    return 'Never used'
  }
  const parsed = new Date(timestamp)
  if (Number.isNaN(parsed.getTime())) {
    return 'Never used'
  }
  return `Last used ${formatDistanceToNow(parsed, { addSuffix: true })}`
}

function ProfileSwatch({ color }: { color: string }) {
  return (
    <div
      aria-hidden
      className="relative h-11 w-11 shrink-0 rounded-xl shadow-[inset_0_1px_0_rgba(255,255,255,0.3),inset_0_0_0_1px_rgba(0,0,0,0.06),0_2px_6px_-3px_rgba(0,0,0,0.18)] after:absolute after:inset-1 after:rounded-[10px] after:bg-[linear-gradient(160deg,rgba(255,255,255,0.18),transparent_60%)] after:content-['']"
      style={{ background: color }}
    />
  )
}
