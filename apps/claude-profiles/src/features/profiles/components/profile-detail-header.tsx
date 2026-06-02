import type { ReactNode } from 'react'
import type { AppId } from '@/lib/app-registry'

import { ariaKeyshortcutsFor, Button, Kbd } from '@/design'

import { BrandMark } from './brand-mark'

type Props = {
  name: string
  swatch: ReactNode
  subline?: ReactNode
  onEdit?: () => void
}

/**
 * Detail-page header chrome. The caller supplies the swatch (colour-filled
 * for managed profiles, grey-outlined for the default entry) and the
 * sub-line (slug + last-used for managed, bare command name for default).
 * The Edit button only renders when `onEdit` is provided.
 */
export function ProfileDetailHeader({ name, swatch, subline, onEdit }: Props) {
  return (
    <header className="mb-5 flex items-start gap-4 border-b border-border-soft pb-5">
      {swatch}
      <div className="min-w-0 flex-1 pt-px">
        <h2 className="m-0 mb-1 text-display font-bold tracking-[-0.03em] text-ink leading-[1.05]">{name}</h2>
        {subline ? <p className="font-mono text-[12px] tracking-[-0.005em] text-muted">{subline}</p> : null}
      </div>
      {onEdit ? (
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
      ) : null}
    </header>
  )
}

export function ProfileSwatch({ color }: { color: string }) {
  return (
    <div
      aria-hidden
      className="relative h-11 w-11 shrink-0 rounded-xl shadow-[inset_0_1px_0_rgba(255,255,255,0.3),inset_0_0_0_1px_rgba(0,0,0,0.06),0_2px_6px_-3px_rgba(0,0,0,0.18)] after:absolute after:inset-1 after:rounded-[10px] after:bg-[linear-gradient(160deg,rgba(255,255,255,0.18),transparent_60%)] after:content-['']"
      style={{ background: color }}
    />
  )
}

/**
 * Swatch for a default (stock-install) entry: the real vendor brand logo on a
 * neutral tile, matching the 44px footprint of `ProfileSwatch` so the header
 * layout is identical between managed and default profiles.
 */
export function BrandSwatch({ app }: { app: AppId }) {
  return (
    <div
      aria-hidden
      className="grid h-11 w-11 shrink-0 place-items-center rounded-xl bg-white shadow-[inset_0_0_0_1px_rgba(0,0,0,0.06),0_2px_6px_-3px_rgba(0,0,0,0.18)] dark:bg-cream-2"
    >
      <BrandMark app={app} size={26} />
    </div>
  )
}
