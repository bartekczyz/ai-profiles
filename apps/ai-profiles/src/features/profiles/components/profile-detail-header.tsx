import type { ReactNode } from 'react'
import type { AppId } from '@/lib/app-registry'

import { ariaKeyshortcutsFor } from '@/design'

import { BrandMark } from './brand-mark'

type Props = {
  name: string
  swatch: ReactNode
  /**
   * Control that leads the action group in place of Edit, for entries that
   * have no Edit to offer. Style it with `headerControlClasses` so it joins
   * the group rather than floating beside it.
   */
  action?: ReactNode
  info?: ReactNode
  subline?: ReactNode
  menu?: ReactNode
  onEdit?: () => void
}

/**
 * Outlined chrome shared by every control in the header's action group.
 * Exported so the overflow-menu trigger is styled identically to Edit: the
 * two have to read as one control group split by a hairline, not as two
 * unrelated buttons. Outlined rather than ghost because a bare "…" glyph is
 * invisible until you already know it is there, and it is the only route to
 * the reveal paths — and, on a managed profile, to Delete.
 *
 * Deliberately carries no border radius — the group container rounds its
 * first and last child, so a lone control still reads as a normal button.
 */
export const headerControlClasses =
  'inline-flex h-[26px] shrink-0 cursor-pointer items-center justify-center gap-1.5 border border-border bg-cream px-2.5 text-[11.5px] font-medium leading-none text-ink-soft outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:not-disabled:border-border-strong hover:not-disabled:bg-white hover:not-disabled:text-ink focus-visible:relative focus-visible:z-10 focus-visible:ring-2 focus-visible:ring-orange/40 disabled:cursor-not-allowed disabled:opacity-60 dark:bg-white/[0.04] dark:hover:not-disabled:bg-white/[0.07]'

/**
 * Detail-page header chrome. The caller supplies the swatch (colour-filled
 * for managed profiles, the vendor brand mark for the stock install), the
 * sub-line (app + last-used for managed, what the entry is for the stock
 * install), the information affordance that rides beside the name, and the
 * controls that make up the action group on the right.
 *
 * Every slot is optional, so each entry fills the group with what it can
 * actually do rather than with disabled stand-ins: a managed profile pairs
 * Edit with the overflow menu, the stock install swaps Edit for Import (via
 * `action`) and carries a menu holding only the reveal destinations. Supply
 * `onEdit` or `action`, never both — they occupy the same position. With
 * nothing at all the group is dropped rather than left as an empty gap.
 *
 * Edit carries no keyboard chip — chips are reserved for primary actions —
 * but the shortcut is still announced via `aria-keyshortcuts`.
 */
export function ProfileDetailHeader({ name, swatch, action, info, subline, menu, onEdit }: Props) {
  const hasActions = onEdit !== undefined || action !== undefined || menu !== undefined
  return (
    <header className="flex items-center gap-3.5 border-b border-border-soft pb-5">
      {swatch}
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          <h2 className="m-0 text-display font-bold tracking-[-0.03em] text-ink leading-[1.05]">{name}</h2>
          {info}
        </div>
        {subline ? <p className="mt-1 font-mono text-[12px] tracking-[-0.005em] text-muted">{subline}</p> : null}
      </div>
      {hasActions ? (
        // One outlined container split by a hairline: the children carry no
        // radius of their own, the group rounds its ends, and the overlap
        // (-ml-px) collapses two adjacent borders into a single line.
        <div className="inline-flex shrink-0 items-center [&>*:first-child]:rounded-l-md [&>*:last-child]:rounded-r-md [&>*+*]:-ml-px">
          {onEdit ? (
            <button
              type="button"
              className={headerControlClasses}
              aria-keyshortcuts={ariaKeyshortcutsFor('edit-selected')}
              onClick={onEdit}
            >
              Edit
            </button>
          ) : null}
          {action}
          {menu}
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
