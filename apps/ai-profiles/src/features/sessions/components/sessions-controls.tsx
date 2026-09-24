import type { SegmentedOption } from '@/design'
import type { KindFilter, SortDirection } from '../lib/session-filters'

import { ArrowDownWideNarrow, ArrowUpNarrowWide, Monitor, Search, Terminal } from 'lucide-react'

import { Segmented, TooltipBubble } from '@/design'
import { Input } from '@/design/ui/input'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/design/ui/tooltip'

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

/**
 * The kind filter's segments, in order: All in words, the kinds as the icons
 * their rows' badges carry, each named for a screen reader and on hover.
 */
const kindOptions: ReadonlyArray<SegmentedOption<KindFilter>> = [
  { value: 'all', label: 'All' },
  {
    value: 'desktop',
    ariaLabel: 'Desktop',
    label: (
      <>
        <Monitor aria-hidden className="h-3.5 w-3.5" />
        <TooltipBubble>Desktop only</TooltipBubble>
      </>
    ),
  },
  {
    value: 'cli',
    ariaLabel: 'CLI',
    label: (
      <>
        <Terminal aria-hidden className="h-3.5 w-3.5" />
        <TooltipBubble>CLI only</TooltipBubble>
      </>
    ),
  },
]

/**
 * What the sort toggle says for each order.
 */
const directionLabels: Record<SortDirection, string> = {
  desc: 'Newest first',
  asc: 'Oldest first',
}

/**
 * The toolbar under the panel's header that narrows and orders the open tab:
 * search, the kind filter, and the last-used sort. One compact line — the
 * search takes the room left, the filter and the sort keep to icons.
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
    <div className="flex shrink-0 items-center gap-1.5 px-[13px] pt-2 pb-[9px]">
      <div className="relative min-w-0 flex-1">
        <Search
          aria-hidden
          className="pointer-events-none absolute top-1/2 left-[9px] h-[13px] w-[13px] -translate-y-1/2 text-muted-strong"
        />
        <Input
          type="search"
          aria-label="Search sessions"
          placeholder="Search sessions"
          title="Search title, folder, prompt"
          value={query}
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck={false}
          className="h-7 rounded-[7px] py-0 pl-7 text-[12px]"
          onChange={(event) => onQueryChange(event.target.value)}
        />
      </div>
      {kindFilterShown ? (
        <Segmented ariaLabel="Session kind" size="small" options={kindOptions} value={kind} onChange={onKindChange} />
      ) : null}
      {/* Portalled, unlike the filter's bubbles: the button sits at the
          card's right edge, and a bubble centred on it would be clipped. */}
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              type="button"
              aria-label={`Last used, ${directionLabels[direction].toLowerCase()}`}
              className="inline-flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-[7px] border border-border bg-white text-muted outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:border-border-strong hover:text-ink focus-visible:ring-2 focus-visible:ring-orange/40 dark:bg-cream-2"
              onClick={() => onDirectionChange(direction === 'desc' ? 'asc' : 'desc')}
            >
              <SortIcon aria-hidden className="h-3.5 w-3.5" />
            </button>
          </TooltipTrigger>
          <TooltipContent sideOffset={4} className="px-2 py-1 font-mono text-[11px] leading-[1.4]">
            Last used · {directionLabels[direction].toLowerCase()}
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    </div>
  )
}
