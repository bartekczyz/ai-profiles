import type { SegmentedOption } from '@/design'
import type { KindFilter, SortDirection } from '../lib/session-filters'

import { ArrowDownWideNarrow, ArrowUpNarrowWide, Search } from 'lucide-react'

import { Segmented, TooltipBubble } from '@/design'
import { Input } from '@/design/ui/input'

type Props = {
  /**
   * Whether the kind filter is offered — only when the open tab mixes desktop
   * and CLI sessions, since otherwise it could only hide everything.
   */
  kindFilterShown: boolean
  /**
   * The search text.
   */
  query: string
  /**
   * The kind the list is narrowed to.
   */
  kind: KindFilter
  /**
   * The last-used order.
   */
  direction: SortDirection
  /**
   * Called with the new search text.
   */
  onQueryChange: (query: string) => void
  /**
   * Called with the newly chosen kind.
   */
  onKindChange: (kind: KindFilter) => void
  /**
   * Called with the order to switch to.
   */
  onDirectionChange: (direction: SortDirection) => void
}

const kindOptions: ReadonlyArray<SegmentedOption<KindFilter>> = [
  { value: 'all', label: 'All' },
  { value: 'desktop', label: 'Desktop' },
  { value: 'cli', label: 'CLI' },
]

const directionLabels: Record<SortDirection, string> = {
  desc: 'Newest first',
  asc: 'Oldest first',
}

/**
 * The row under the tabs that narrows and orders the open tab: search, the
 * kind filter, and the last-used sort. It wraps rather than squeezes when the
 * panel is narrow, the search keeping the most room.
 */
export function SessionsControls({
  kindFilterShown,
  query,
  kind,
  direction,
  onQueryChange,
  onKindChange,
  onDirectionChange,
}: Props) {
  const SortIcon = direction === 'desc' ? ArrowDownWideNarrow : ArrowUpNarrowWide
  return (
    <div className="flex flex-wrap items-center gap-2">
      <div className="relative min-w-40 flex-1">
        <Search
          aria-hidden
          className="pointer-events-none absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2 text-muted-strong"
        />
        <Input
          type="search"
          aria-label="Search sessions"
          placeholder="Search title, folder, prompt"
          value={query}
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck={false}
          className="h-9 py-0 pl-8 text-[12.5px]"
          onChange={(event) => onQueryChange(event.target.value)}
        />
      </div>
      {kindFilterShown ? (
        <Segmented ariaLabel="Session kind" options={kindOptions} value={kind} onChange={onKindChange} />
      ) : null}
      <button
        type="button"
        aria-label={`Last used, ${directionLabels[direction].toLowerCase()}`}
        className="group relative inline-flex h-9 shrink-0 cursor-pointer items-center gap-1.5 rounded-md border border-border bg-white px-2.5 text-[12px] text-muted outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:border-border-strong hover:text-ink focus-visible:ring-2 focus-visible:ring-orange/40 dark:bg-cream-2"
        onClick={() => onDirectionChange(direction === 'desc' ? 'asc' : 'desc')}
      >
        <SortIcon aria-hidden className="h-3.5 w-3.5" />
        Last used
        <TooltipBubble>{directionLabels[direction]}</TooltipBubble>
      </button>
    </div>
  )
}
