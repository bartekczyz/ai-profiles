import type { SegmentedOption } from '@/design'
import type { KindFilter, SessionSort, SortDirection, SortKey } from '../lib/session-filters'

import { Fragment } from 'react'

import {
  CaretDown,
  MagnifyingGlass,
  Monitor,
  SortAscending,
  SortDescending,
  TerminalWindow,
} from '@phosphor-icons/react'

import { Segmented, TooltipBubble } from '@/design'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/design/ui/dropdown-menu'
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
   * The order of the rows.
   */
  sort: SessionSort
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
  onSortChange: (sort: SessionSort) => void
}

/**
 * One thing the list can be ordered by, as the sort menu offers it.
 */
type SortChoice = {
  /**
   * What it orders by.
   */
  key: SortKey
  /**
   * Its name.
   */
  label: string
  /**
   * What each way it runs is called.
   */
  directions: Record<SortDirection, string>
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
        <TerminalWindow aria-hidden className="h-3.5 w-3.5" />
        <TooltipBubble>CLI only</TooltipBubble>
      </>
    ),
  },
]

/**
 * What the list can be ordered by, in the sort menu's order.
 */
export const sortChoices: ReadonlyArray<SortChoice> = [
  { key: 'lastUsed', label: 'Last used', directions: { desc: 'Newest first', asc: 'Oldest first' } },
  { key: 'cost', label: 'API-equivalent', directions: { desc: 'Highest first', asc: 'Lowest first' } },
]

/**
 * Each way the list can run, in the sort menu's order.
 */
const directions: ReadonlyArray<SortDirection> = ['desc', 'asc']

/**
 * The toolbar under the panel's header that narrows and orders the open tab:
 * search, the kind filter, and the sort. One compact line — the search takes
 * the room left, the filter keeps to icons, and the sort names what it orders
 * by only where the panel has room.
 */
export function SessionsControls({
  kindFilterShown,
  query,
  kind,
  sort,
  onQueryChange,
  onKindChange,
  onSortChange,
}: Props) {
  const SortIcon = sort.direction === 'desc' ? SortDescending : SortAscending
  const choice = sortChoices.find((each) => each.key === sort.key) ?? sortChoices[0]
  return (
    <div className="flex shrink-0 items-center gap-1.5 px-[13px] pt-2 pb-[9px]">
      <div className="relative min-w-0 flex-1">
        <MagnifyingGlass
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
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            aria-label={`Sort: ${choice.label}, ${choice.directions[sort.direction].toLowerCase()}`}
            className="inline-flex h-7 min-w-7 shrink-0 items-center justify-center gap-1.5 rounded-[7px] border border-border bg-white px-[7px] text-[12px] text-muted outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:border-border-strong hover:text-ink focus-visible:ring-2 focus-visible:ring-orange/40 data-[state=open]:border-border-strong data-[state=open]:text-ink dark:bg-cream-2"
          >
            <SortIcon aria-hidden className="h-3.5 w-3.5" />
            <span className="hidden @min-[480px]/sessions:inline">{choice.label}</span>
            <CaretDown aria-hidden className="hidden h-3 w-3 opacity-70 @min-[480px]/sessions:block" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-44">
          <DropdownMenuRadioGroup
            value={`${sort.key}:${sort.direction}`}
            onValueChange={(value) => onSortChange(sortOf(value))}
          >
            {sortChoices.map((each, index) => (
              <Fragment key={each.key}>
                {index > 0 ? <DropdownMenuSeparator /> : null}
                <DropdownMenuLabel className="text-[11px] text-muted-strong">{each.label}</DropdownMenuLabel>
                {directions.map((direction) => (
                  <DropdownMenuRadioItem
                    key={direction}
                    value={`${each.key}:${direction}`}
                    className="px-2 py-1.5 text-[12px]"
                  >
                    {each.directions[direction]}
                  </DropdownMenuRadioItem>
                ))}
              </Fragment>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  )
}

/**
 * The order a sort menu item's `value`, `<key>:<direction>`, stands for.
 */
function sortOf(value: string): SessionSort {
  const [key, direction] = value.split(':')
  return {
    key: key === 'cost' ? 'cost' : 'lastUsed',
    direction: direction === 'asc' ? 'asc' : 'desc',
  }
}
