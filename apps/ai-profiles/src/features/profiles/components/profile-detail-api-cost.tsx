import type { AppId } from '@/lib/app-registry'
import type { ApiCost, ModelCost, Period } from '@/lib/types'

import { useState } from 'react'

import { CaretRight } from '@phosphor-icons/react'
import { format, parseISO } from 'date-fns'

import { Button, cn, Dialog, panelClasses, Segmented, Skeleton } from '@/design'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/design/ui/tooltip'
import { formatShare, formatUsd } from '@/lib/format-cost'

import { useProfileApiCost } from '../api/use-profile-api-cost'
import { useProfileUsageSnapshot } from '../api/use-profile-usage'
import { type LimitCycle, limitCycles } from '../lib/limit-cycle'
import { shortenHomePath } from './shorten-home-path'

/**
 * Bar colours of the models, most expensive first. Past the last, it repeats.
 */
const modelColors = ['bg-profile-orange', 'bg-profile-steel', 'bg-profile-sage', 'bg-profile-mauve', 'bg-profile-amber']

/**
 * How many projects the breakdown names before summing up the rest.
 */
const shownProjects = 8

/**
 * A cycle of the profile's longest limit, offered once the usage says when
 * that resets.
 */
type CyclePeriod = 'thisCycle' | 'lastCycle'
type CostPeriod = Period | CyclePeriod

const cyclePeriods: ReadonlyArray<{ value: CyclePeriod; label: string; length?: string }> = [
  { value: 'thisCycle', label: 'This cycle' },
  { value: 'lastCycle', label: 'Last cycle' },
]

/**
 * The periods of days the estimate can cover, as the picker names them, with
 * the length of each one counted back from today.
 */
const dayPeriods: ReadonlyArray<{ value: Period; label: string; length?: string }> = [
  { value: 'today', label: 'Today' },
  { value: 'yesterday', label: 'Yesterday' },
  { value: 'last7Days', label: '7 days', length: '7 days' },
  { value: 'last30Days', label: '30 days', length: '30 days' },
  { value: 'last90Days', label: '90 days', length: '90 days' },
  { value: 'last6Months', label: '6 months', length: '6 months' },
  { value: 'year', label: 'Year', length: '12 months' },
]

const defaultPeriod: Period = 'last30Days'
const periodKey = 'ai-profiles-api-cost-period'

function readPeriod(): Period {
  try {
    const stored = window.localStorage.getItem(periodKey)
    return dayPeriods.find((each) => each.value === stored)?.value ?? defaultPeriod
  } catch {
    return defaultPeriod
  }
}

function isCycle(period: CostPeriod): period is CyclePeriod {
  return cyclePeriods.some((each) => each.value === period)
}

function periodOf(period: CostPeriod): { label: string; length?: string } {
  return [...cyclePeriods, ...dayPeriods].find((each) => each.value === period) ?? { label: period }
}

/**
 * What the line and the summary call `period`: "Today", "Last 7 days".
 */
function periodName(period: CostPeriod): string {
  const { label, length } = periodOf(period)
  return length ? `Last ${length}` : label
}

/**
 * Per app: the tool whose transcripts the estimate reads, whose API prices it
 * is at, what its cache writes are called, and the chats it can't see.
 */
const sources: Record<AppId, { tool: string; api: string; cacheWrites: string; leftOut: string; kept?: string }> = {
  claude: {
    tool: 'Claude Code',
    api: 'Claude API',
    cacheWrites: 'Cache writes, 5 min',
    leftOut: "Chats in the Claude desktop app and on claude.ai keep no token counts, so they aren't included.",
    kept: 'Claude Code deletes transcripts after 30 days unless cleanupPeriodDays in its settings says otherwise, so older use is gone.',
  },
  codex: {
    tool: 'Codex',
    api: 'OpenAI API',
    cacheWrites: 'Cache writes',
    leftOut: 'Only Codex use is included, in the CLI and the desktop app: ChatGPT chats keep no token counts here.',
  },
}

const eyebrowClasses = 'mb-2 text-[12.5px] font-medium text-muted'

/**
 * A card beside the launch panel: a label line over the figure. `flex-1` with
 * a zero basis so it takes the third of the row the launch panel leaves it,
 * and the whole row when it renders nothing. Under the launch row's
 * breakpoint the two stack and it takes the full width.
 */
const rowClasses = cn(
  panelClasses,
  'flex min-w-0 flex-col justify-between gap-3 px-4 py-3 text-left text-meta @[620px]:flex-1 @[620px]:basis-0',
)

const rowLabelClasses = 'text-[12.5px] font-medium text-ink-soft'

type Props = {
  app: AppId
  profileId: string
}

/**
 * A card estimating what the profile's Claude Code or Codex use would cost at
 * API prices over the period of days last picked and, once the usage says when
 * its limit resets, over the cycle running now. It opens the breakdown, where
 * a past cycle can be looked at too. Nothing for a profile without transcripts
 * yet. Until the transcripts are read, the card holds its place, so what
 * follows doesn't jump down when it arrives. The period of days is remembered
 * for every profile; a cycle picked in the breakdown lasts until the app
 * closes and leaves the card as it is.
 *
 * It is an estimate of the recorded use, never the bill: the profile may pay
 * by API key, by subscription, or both over time, and nothing on disk says
 * which, so it isn't compared against a plan.
 */
export function ProfileDetailApiCost({ app, profileId }: Props) {
  const { cycle, cyclePeriod, cycles, days, changePeriod } = useCostPeriods(app, profileId)
  const { data: daysCost, isError } = useProfileApiCost(profileId, days)
  const { data: currentCycleCost } = useProfileApiCost(profileId, cycles?.current ?? null)
  const { data: shownCost } = useProfileApiCost(profileId, cycle ?? days)
  if (!daysCost) {
    return <CostPlaceholder isError={isError} />
  }
  if (!daysCost.hasUse) {
    return null
  }
  return (
    <ApiCostRow
      app={app}
      cost={shownCost ?? daysCost}
      currentCycleCost={cycles ? currentCycleCost : null}
      cycle={cycle}
      days={days}
      daysCost={daysCost}
      hasCycles={cycles !== null}
      period={cyclePeriod ?? days}
      onPeriodChange={changePeriod}
    />
  )
}

/**
 * The period of days remembered for every profile, the cycle picked in the
 * breakdown, if the usage says when the profile's cycles run, and how to
 * change them.
 */
function useCostPeriods(app: AppId, profileId: string) {
  const [days, setDays] = useState<Period>(readPeriod)
  const [pickedCycle, setPickedCycle] = useState<CyclePeriod | null>(null)
  const { data: usage } = useProfileUsageSnapshot(profileId)
  const cycles = limitCycles(app, usage?.quota ?? null, new Date())
  const cyclePeriod = cycles ? pickedCycle : null
  function changePeriod(next: CostPeriod) {
    if (isCycle(next)) {
      setPickedCycle(next)
      return
    }
    setPickedCycle(null)
    setDays(next)
    try {
      window.localStorage.setItem(periodKey, next)
    } catch {
      // Storage may be unavailable; the pick still holds for this session.
    }
  }
  return { cycle: cycleOf(cycles, cyclePeriod), cyclePeriod, cycles, days, changePeriod }
}

function cycleOf(cycles: ReturnType<typeof limitCycles>, period: CyclePeriod | null): LimitCycle | null {
  if (!cycles || !period) {
    return null
  }
  return period === 'thisCycle' ? cycles.current : cycles.previous
}

function CostPlaceholder({ isError }: { isError: boolean }) {
  return (
    <div className={rowClasses}>
      <span className={rowLabelClasses}>Estimated API cost</span>
      {isError ? (
        <span className="text-muted">Couldn't read the transcripts</span>
      ) : (
        // The zero-width space gives the row the line height of the text
        // that replaces the skeleton, so the row keeps its height.
        <>
          <Skeleton shape="text" className="w-28" />
          &#8203;
        </>
      )}
    </div>
  )
}

type RowProps = {
  app: AppId
  /**
   * The estimate of the period picked in the breakdown.
   */
  cost: ApiCost
  /**
   * The estimate of the cycle running now: null without one, undefined until
   * it is read.
   */
  currentCycleCost: ApiCost | null | undefined
  /**
   * The limit cycle `period` stands for, if it is one.
   */
  cycle: LimitCycle | null
  /**
   * The period of days the card shows, whatever the breakdown shows.
   */
  days: Period
  /**
   * The estimate of `days`.
   */
  daysCost: ApiCost
  /**
   * Whether the picker offers the cycles.
   */
  hasCycles: boolean
  period: CostPeriod
  onPeriodChange: (period: CostPeriod) => void
}

function ApiCostRow({
  app,
  cost,
  currentCycleCost,
  cycle,
  days,
  daysCost,
  hasCycles,
  period,
  onPeriodChange,
}: RowProps) {
  const [open, setOpen] = useState(false)
  const source = sources[app]

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className={`${rowClasses} transition-colors hover:bg-cream focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring dark:hover:bg-white/[0.05]`}
      >
        <span className="flex w-full items-center gap-2">
          <span className={rowLabelClasses}>Estimated API cost</span>
          <CaretRight aria-hidden size={14} className="ml-auto shrink-0 text-muted-strong" />
        </span>
        <span className="grid auto-cols-fr grid-flow-col divide-x divide-border-soft">
          <CostFigure cost={daysCost} label={periodName(days)} />
          {currentCycleCost === null ? null : <CostFigure cost={currentCycleCost} label={periodName('thisCycle')} />}
        </span>
      </button>
      <Dialog
        open={open}
        title="Estimated API cost"
        description={`What ${source.tool} use on this profile would cost at ${source.api} prices. An estimate, not your bill.`}
        // Held at the top, not centred: each period is a different height,
        // and centred, the picker would move out from under the pointer.
        className="top-8 w-[min(780px,calc(100%-64px))] translate-y-0"
        onClose={() => setOpen(false)}
        foot={
          <Button variant="ghost" size="sm" shortcut="Escape" onClick={() => setOpen(false)}>
            Close
          </Button>
        }
      >
        <div className="mb-4 flex flex-wrap gap-2">
          {hasCycles ? (
            <Segmented<CostPeriod>
              size="small"
              ariaLabel="Limit cycle"
              options={cyclePeriods}
              value={period}
              onChange={onPeriodChange}
            />
          ) : null}
          <Segmented<CostPeriod>
            size="small"
            ariaLabel="Days"
            options={dayPeriods}
            value={period}
            onChange={onPeriodChange}
          />
        </div>
        <Summary cost={cost} cycle={cycle} period={period} kept={source.kept} />
        {cost.unpriced.length > 0 ? (
          <p className="m-0 mb-5 text-meta text-amber">{unpricedNote(cost.unpriced)}</p>
        ) : null}
        {cost.byModel.length === 0 ? null : (
          <>
            <CostOverTime cost={cost} />
            <div className="grid grid-cols-2 gap-6">
              <KindBreakdown byKind={cost.byKind} cacheWrites={source.cacheWrites} />
              <Projects projects={cost.projects} />
            </div>
          </>
        )}
        <Footnote cost={cost} app={app} />
      </Dialog>
    </>
  )
}

/**
 * A figure of the card under what it covers, with a dash until it is read.
 * The figures share the card in equal columns, a hairline apart.
 */
function CostFigure({ cost, label }: { cost: ApiCost | undefined; label: string }) {
  const unpriced = cost?.coverage === 'unpriced'
  return (
    <span className="flex min-w-0 flex-col gap-0.5 px-4 first:pl-0 last:pr-0">
      <span className="truncate text-muted">
        {label}
        {cost?.coverage === 'partial' ? <span className="text-amber"> · partial</span> : null}
      </span>
      <span
        className={`text-[20px] leading-tight font-semibold tracking-[-0.015em] tabular-nums ${unpriced ? 'text-amber' : 'text-ink'}`}
      >
        {unpriced ? 'Unpriced' : cost ? formatUsd(cost.totalUsd) : '—'}
      </span>
    </span>
  )
}

/**
 * The total and the days it covers, with how long the tool keeps transcripts,
 * `kept`, when they don't reach back to the start of a period over 30 days.
 */
function Summary({
  cost,
  cycle,
  period,
  kept,
}: {
  cost: ApiCost
  cycle: LimitCycle | null
  period: CostPeriod
  kept?: string
}) {
  return (
    <section className="mb-4">
      {cost.coverage === 'unpriced' ? (
        <p className="m-0 text-[28px] font-semibold tracking-[-0.02em] text-amber">Unpriced</p>
      ) : (
        <p className="m-0 text-[28px] font-semibold tracking-[-0.02em] text-ink tabular-nums">
          {formatUsd(cost.totalUsd)}
          {cost.coverage === 'partial' ? (
            <span className="ml-2 align-middle font-mono text-mono font-medium tracking-normal text-amber">
              partial
            </span>
          ) : null}
        </p>
      )}
      <p className="m-0 text-meta text-muted">{summaryText(cost, period)}</p>
      {cycle ? <p className="m-0 mt-1 text-meta text-muted">{cycleText(cycle, new Date())}</p> : null}
      {kept && cost.days.length > 30 && cost.coveredDays < cost.days.length ? (
        <p className="m-0 mt-1 text-meta text-muted">{kept}</p>
      ) : null}
    </section>
  )
}

/**
 * The chart over a period of days, or for a single day, what each model cost.
 */
function CostOverTime({ cost }: { cost: ApiCost }) {
  if (cost.days.length > 1) {
    return <CostChart cost={cost} />
  }
  if (cost.byModel.length === 0) {
    return null
  }
  return (
    <section className="mb-5">
      <h3 className={eyebrowClasses}>By model</h3>
      <ModelLegend byModel={cost.byModel} />
    </section>
  )
}

type Bar = { from: string; to: string; totalUsd: number; byModel: Array<ModelCost> }

/**
 * The cost of each day the transcripts cover, or of each week when they cover
 * over 90 days, as daily bars would be too thin to point at: the days before
 * the oldest one are left out rather than drawn as days that cost nothing,
 * so a year that the transcripts cover a month of is drawn by day. Pointing
 * at a bar shows its days and what each model cost then.
 */
function CostChart({ cost }: { cost: ApiCost }) {
  const covered = cost.days.slice(cost.days.length - cost.coveredDays)
  const daysPerBar = covered.length > 90 ? 7 : 1
  const unit = daysPerBar === 1 ? 'day' : 'week'
  const bars = barsOf(covered, daysPerBar, cost.byModel)
  const highest = Math.max(0, ...bars.map((bar) => bar.totalUsd))
  const first = bars[0]
  const lastDay = cost.days[cost.days.length - 1].date
  return (
    <section className="mb-5">
      <div className="flex items-baseline justify-between">
        <h3 className={eyebrowClasses}>By {unit}</h3>
        <span className="font-mono text-mono text-muted-strong">
          peak {formatUsd(highest)}/{unit}
        </span>
      </div>
      <TooltipProvider>
        <div
          role="img"
          aria-label={`Cost per ${unit} over ${dayCount(cost.coveredDays)}`}
          className="flex h-28 items-end gap-[3px] border-b border-border-soft"
        >
          {bars.map((bar) => (
            <Tooltip key={bar.from}>
              <TooltipTrigger asChild>
                <div className="flex h-full flex-1 flex-col justify-end rounded-t-[2px] hover:bg-ink/[0.05]">
                  <div
                    className="flex flex-col-reverse overflow-hidden rounded-t-[2px]"
                    style={{ height: `${highest > 0 ? (bar.totalUsd / highest) * 100 : 0}%` }}
                  >
                    {bar.byModel.map((model) => (
                      <div
                        key={model.model}
                        className={colorOf(cost.byModel, model.model)}
                        style={{ height: `${(model.usd / bar.totalUsd) * 100}%` }}
                      />
                    ))}
                  </div>
                </div>
              </TooltipTrigger>
              <TooltipContent
                side="top"
                sideOffset={4}
                className="flex-col items-stretch gap-1 px-2.5 py-2 font-mono text-[11px] leading-[1.4]"
              >
                <BarBreakdown bar={bar} byModel={cost.byModel} />
              </TooltipContent>
            </Tooltip>
          ))}
        </div>
      </TooltipProvider>
      <div className="mt-1 flex justify-between font-mono text-mono text-muted-strong">
        <span>{first ? formatDay(first.from) : null}</span>
        <span>{lastDay === format(new Date(), 'yyyy-MM-dd') ? 'Today' : formatDay(lastDay)}</span>
      </div>
      <ModelLegend byModel={cost.byModel} className="mt-2.5" />
    </section>
  )
}

/**
 * A bar's days, its total, and what each model cost then, most first.
 */
function BarBreakdown({ bar, byModel }: { bar: Bar; byModel: Array<ModelCost> }) {
  return (
    <>
      <span className="flex justify-between gap-4">
        <span>{barLabel(bar)}</span>
        <span className="tabular-nums">{formatUsd(bar.totalUsd)}</span>
      </span>
      {[...bar.byModel]
        .sort((left, right) => right.usd - left.usd)
        .map((model) => (
          <span key={model.model} className="flex items-center gap-1.5">
            <span aria-hidden className={`h-2 w-2 rounded-[2px] ${colorOf(byModel, model.model)}`} />
            {model.model}
            <span className="ml-auto pl-4 tabular-nums">{formatUsd(model.usd)}</span>
          </span>
        ))}
    </>
  )
}

function ModelLegend({ byModel, className = '' }: { byModel: Array<ModelCost>; className?: string }) {
  return (
    <ul className={`m-0 flex list-none flex-wrap gap-x-4 gap-y-1 p-0 text-meta text-muted ${className}`}>
      {byModel.map((model) => (
        <li key={model.model} className="flex items-center gap-1.5">
          <span aria-hidden className={`h-2 w-2 rounded-[2px] ${colorOf(byModel, model.model)}`} />
          {model.model}
          <span className="text-ink tabular-nums">{formatUsd(model.usd)}</span>
        </li>
      ))}
    </ul>
  )
}

/**
 * The bar colour of `model`, by its place in `byModel`, most expensive first.
 */
function colorOf(byModel: Array<ModelCost>, model: string): string {
  return modelColors[
    Math.min(
      byModel.findIndex((each) => each.model === model),
      modelColors.length - 1,
    )
  ]
}

/**
 * `days` in bars of `size` days each, the last ending on the last day, so only
 * the first may be shorter. A bar's models are in `byModel`'s order.
 */
export function barsOf(days: ApiCost['days'], size: number, byModel: Array<ModelCost>): Array<Bar> {
  if (days.length === 0) {
    return []
  }
  const firstSize = days.length % size || size
  const chunks = [days.slice(0, firstSize)]
  for (let start = firstSize; start < days.length; start += size) {
    chunks.push(days.slice(start, start + size))
  }
  return chunks.map((chunk) => {
    const spentOn = (model: string) =>
      chunk.reduce((sum, day) => sum + (day.byModel.find((each) => each.model === model)?.usd ?? 0), 0)
    return {
      from: chunk[0].date,
      to: chunk[chunk.length - 1].date,
      totalUsd: chunk.reduce((sum, day) => sum + day.totalUsd, 0),
      byModel: byModel.map(({ model }) => ({ model, usd: spentOn(model) })).filter((each) => each.usd > 0),
    }
  })
}

/**
 * A day as "Thu, Oct 15", a week as "Oct 9 – Oct 15".
 */
function barLabel(bar: Bar): string {
  return bar.from === bar.to
    ? format(parseISO(bar.from), 'EEE, MMM d')
    : `${formatDay(bar.from)} – ${formatDay(bar.to)}`
}

function KindBreakdown({ byKind, cacheWrites }: { byKind: ApiCost['byKind']; cacheWrites: string }) {
  const rows = kindRows(byKind, cacheWrites)
  const total = rows.reduce((sum, row) => sum + row.usd, 0)
  return (
    <section className="mb-5 min-w-0">
      <h3 className={eyebrowClasses}>By what was billed</h3>
      <CostRows rows={rows.map((row) => ({ ...row, key: row.label, share: total > 0 ? row.usd / total : 0 }))} />
    </section>
  )
}

function Projects({ projects }: { projects: ApiCost['projects'] }) {
  const shown = projects.slice(0, shownProjects)
  const rest = projects.slice(shownProjects)
  const names = shown.map((project) => (project.path === null ? 'Unknown folder' : folderName(project.path)))
  const rows: Array<CostRow> = shown.map((project, index) => {
    const name = names[index]
    if (project.path === null) {
      return { key: 'unknown', label: name, usd: project.usd }
    }
    // Folders that share a name are told apart by their paths.
    const path = shortenHomePath(project.path)
    const shared = names.indexOf(name) !== names.lastIndexOf(name)
    return { key: project.path, label: shared ? path : name, title: path, usd: project.usd }
  })
  if (rest.length > 0) {
    rows.push({
      key: 'more',
      label: `${rest.length} more`,
      title: undefined,
      usd: rest.reduce((sum, project) => sum + project.usd, 0),
    })
  }
  return (
    <section className="mb-5 min-w-0">
      <h3 className={eyebrowClasses}>By project</h3>
      <CostRows rows={rows} />
    </section>
  )
}

type CostRow = { key: string; label: string; title?: string; usd: number; share?: number }

function CostRows({ rows }: { rows: Array<CostRow> }) {
  return (
    <ul className="m-0 list-none p-0 text-meta">
      {rows.map((row) => (
        <li key={row.key} className="flex items-baseline gap-2 border-b border-border-soft py-1.5 last:border-b-0">
          <span title={row.title} className="min-w-0 flex-1 truncate text-ink-soft">
            {row.label}
          </span>
          {row.share !== undefined ? (
            <span className="font-mono text-mono text-muted-strong tabular-nums">{formatShare(row.share)}</span>
          ) : null}
          <span className="w-20 text-right text-ink tabular-nums">{formatUsd(row.usd)}</span>
        </li>
      ))}
    </ul>
  )
}

function Footnote({ cost, app }: { cost: ApiCost; app: AppId }) {
  return (
    <section className="text-meta text-muted">
      <p className="m-0">
        From this profile's {sources[app].tool} transcripts, at the API list prices in LiteLLM's price list as of{' '}
        {format(parseISO(cost.pricesAsOf), 'MMM d, yyyy')}, whenever the use happened. It estimates the use they record,
        not what you were billed, whether you pay by API key or by subscription. {sources[app].leftOut}
      </p>
    </section>
  )
}

/**
 * The line under the total: the days it covers, the pace when that is only
 * part of `period` (from a week up), and whether the total leaves use without
 * a price out, or there was no use at all.
 */
export function summaryText(
  cost: Pick<ApiCost, 'totalUsd' | 'days' | 'coveredDays' | 'projectedUsd' | 'coverage'>,
  period: CostPeriod,
): string {
  const { label, length } = periodOf(period)
  if (cost.coveredDays === 0) {
    return `Your transcripts don't reach back to ${label.toLowerCase()}.`
  }
  const over =
    cost.coveredDays < cost.days.length
      ? `over the ${dayCount(cost.coveredDays)} your transcripts cover`
      : length
        ? `over the last ${length}`
        : label.toLowerCase()
  if (cost.coverage === 'unpriced') {
    return `None of the use ${over} has a price.`
  }
  if (cost.totalUsd === 0) {
    return `No use ${over}.`
  }
  const pace =
    cost.projectedUsd === null || !length ? '' : ` On pace for ${formatUsd(cost.projectedUsd)} over ${length}.`
  const partial = cost.coverage === 'partial' ? ' Partial: use without a price is left out.' : ''
  return `${over.charAt(0).toUpperCase()}${over.slice(1)}.${pace}${partial}`
}

/**
 * When a limit cycle ran, to the minute: it resets mid-day, so its first and
 * last bars hold only the part of the day inside it.
 */
export function cycleText(cycle: LimitCycle, now: Date): string {
  const moment = (date: Date) => format(date, 'MMM d, HH:mm')
  return cycle.to > now
    ? `Since ${moment(cycle.from)}. Resets ${moment(cycle.to)}.`
    : `${moment(cycle.from)} – ${moment(cycle.to)}.`
}

function dayCount(days: number): string {
  return days === 1 ? '1 day' : `${days} days`
}

function formatDay(date: string): string {
  return format(parseISO(date), 'MMM d')
}

/**
 * The last part of `path`, a folder, or `~` for the home folder.
 */
function folderName(path: string): string {
  const short = shortenHomePath(path)
  return short === '~' ? short : (path.split('/').filter(Boolean).pop() ?? path)
}

/**
 * The billed kinds with any cost, most expensive first, with the cache
 * writes that aren't 1-hour ones called `cacheWrites`.
 */
export function kindRows(byKind: ApiCost['byKind'], cacheWrites: string): Array<{ label: string; usd: number }> {
  const rows = [
    { label: 'Input', usd: byKind.input },
    { label: 'Output', usd: byKind.output },
    { label: cacheWrites, usd: byKind.cacheWrite5m },
    { label: 'Cache writes, 1 hour', usd: byKind.cacheWrite1h },
    { label: 'Cache reads', usd: byKind.cacheRead },
    { label: 'Web search', usd: byKind.webSearch },
  ]
  return rows.filter((row) => row.usd > 0).sort((left, right) => right.usd - left.usd)
}

/**
 * What the price list has no rates for, each model in a sentence of its own.
 * Only a model the list lacks is said to be coming: a new one usually gets
 * its rates soon, while a missing rate, like fast mode's, may never come.
 */
export function unpricedNote(unpriced: ApiCost['unpriced']): string {
  const lacking = unpriced.map(
    (use) => `No ${use.missing ?? 'price'} for ${use.model} (${use.tokens.toLocaleString('en-US')} tokens).`,
  )
  const coming = unpriced.some((use) => use.missing === null)
    ? " LiteLLM's price list may add the models it lacks; it's checked daily."
    : ''
  return `${lacking.join(' ')} They aren't counted.${coming}`
}
