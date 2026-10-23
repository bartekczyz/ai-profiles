import type { AppId } from '@/lib/app-registry'
import type { ApiCost, ModelCost, Period } from '@/lib/types'

import { useState } from 'react'

import { format, parseISO } from 'date-fns'
import { ChevronRight } from 'lucide-react'

import { Button, Dialog, Kbd, Segmented, Skeleton } from '@/design'

import { useProfileApiCost } from '../api/use-profile-api-cost'
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
 * The periods the estimate can cover, as the picker names them, with the
 * length of each one counted back from today.
 */
const periods: ReadonlyArray<{ value: Period; label: string; length?: string }> = [
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
    return periods.find((each) => each.value === stored)?.value ?? defaultPeriod
  } catch {
    return defaultPeriod
  }
}

function periodOf(period: Period): { label: string; length?: string } {
  return periods.find((each) => each.value === period) ?? { label: period }
}

/**
 * What the line and the summary call `period`: "Today", "Last 7 days".
 */
function periodName(period: Period): string {
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

const eyebrowClasses = 'mb-2 font-mono text-[11.5px] font-medium uppercase tracking-[0.08em] text-muted'

const rowClasses =
  'mb-3.5 flex w-full items-center gap-2 rounded-[10px] border border-border-soft bg-white/30 px-[13px] py-[9px] text-left text-meta dark:bg-white/[0.02]'

const rowLabelClasses = 'text-ink-soft'

type Props = {
  app: AppId
  profileId: string
}

/**
 * One line under the usage card estimating what the profile's Claude Code or
 * Codex use over the picked period would cost at API prices, which opens the
 * breakdown. Nothing for a profile without transcripts yet. Until the
 * transcripts are read, the line holds its place, so what follows doesn't
 * jump down when it arrives. The period picked in the breakdown is
 * remembered for every profile.
 *
 * It is an estimate of the recorded use, never the bill: the profile may pay
 * by API key, by subscription, or both over time, and nothing on disk says
 * which, so it isn't compared against a plan.
 */
export function ProfileDetailApiCost({ app, profileId }: Props) {
  const [period, setPeriod] = useState<Period>(readPeriod)
  const { data: cost, isError } = useProfileApiCost(profileId, period)
  function changePeriod(next: Period) {
    setPeriod(next)
    try {
      window.localStorage.setItem(periodKey, next)
    } catch {
      // Storage may be unavailable; the pick still holds for this session.
    }
  }
  if (!cost) {
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
  if (!cost.hasUse) {
    return null
  }
  return <ApiCostRow app={app} cost={cost} period={period} onPeriodChange={changePeriod} />
}

type RowProps = {
  app: AppId
  cost: ApiCost
  period: Period
  onPeriodChange: (period: Period) => void
}

function ApiCostRow({ app, cost, period, onPeriodChange }: RowProps) {
  const [open, setOpen] = useState(false)
  const source = sources[app]

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className={`${rowClasses} cursor-pointer transition-colors hover:bg-white/60 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring dark:hover:bg-white/[0.05]`}
      >
        <span className={rowLabelClasses}>Estimated API cost</span>
        <span className="text-muted">{periodName(period)}</span>
        {cost.coverage === 'unpriced' ? (
          <span className="font-medium text-amber">Unpriced</span>
        ) : (
          <span className="font-medium text-ink tabular-nums">{formatUsd(cost.totalUsd)}</span>
        )}
        {cost.coverage === 'partial' ? <span className="font-mono text-mono text-amber">partial</span> : null}
        <ChevronRight aria-hidden size={14} className="ml-auto shrink-0 text-muted-strong" />
      </button>
      <Dialog
        open={open}
        title="Estimated API cost"
        description={`What ${source.tool} use on this profile would cost at ${source.api} prices. An estimate, not your bill.`}
        className="w-[min(780px,calc(100%-64px))]"
        onClose={() => setOpen(false)}
        foot={
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} onClick={() => setOpen(false)}>
            Close
          </Button>
        }
      >
        <Segmented
          size="small"
          ariaLabel="Period"
          options={periods}
          value={period}
          onChange={onPeriodChange}
          className="mb-4"
        />
        <Summary cost={cost} period={period} kept={source.kept} />
        {cost.unpriced.length > 0 ? (
          <p className="m-0 mb-5 text-meta text-amber">{unpricedNote(cost.unpriced)}</p>
        ) : null}
        {cost.coverage === 'unpriced' ? null : (
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
 * The total and the days it covers, with how long the tool keeps transcripts,
 * `kept`, when they don't reach back to the start of a period over 30 days.
 */
function Summary({ cost, period, kept }: { cost: ApiCost; period: Period; kept?: string }) {
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
 * The cost of each day the transcripts cover, or of each week past 90 days,
 * as daily bars would be too thin to point at: the days before the oldest one
 * are left out rather than drawn as days that cost nothing.
 */
function CostChart({ cost }: { cost: ApiCost }) {
  const daysPerBar = cost.days.length > 90 ? 7 : 1
  const unit = daysPerBar === 1 ? 'day' : 'week'
  const bars = barsOf(cost.days.slice(cost.days.length - cost.coveredDays), daysPerBar, cost.byModel)
  const highest = Math.max(0, ...bars.map((bar) => bar.totalUsd))
  const first = bars[0]
  return (
    <section className="mb-5">
      <div className="flex items-baseline justify-between">
        <h3 className={eyebrowClasses}>By {unit}</h3>
        <span className="font-mono text-mono text-muted-strong">
          peak {formatUsd(highest)}/{unit}
        </span>
      </div>
      <div
        role="img"
        aria-label={`Cost per ${unit} over the last ${dayCount(cost.coveredDays)}`}
        className="flex h-28 items-end gap-[3px] border-b border-border-soft"
      >
        {bars.map((bar) => (
          <div
            key={bar.from}
            title={`${barLabel(bar)}: ${formatUsd(bar.totalUsd)}`}
            className="flex h-full flex-1 flex-col justify-end"
          >
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
        ))}
      </div>
      <div className="mt-1 flex justify-between font-mono text-mono text-muted-strong">
        <span>{first ? formatDay(first.from) : null}</span>
        <span>Today</span>
      </div>
      <ModelLegend byModel={cost.byModel} className="mt-2.5" />
    </section>
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
 * a price out.
 */
export function summaryText(
  cost: Pick<ApiCost, 'days' | 'coveredDays' | 'projectedUsd' | 'coverage'>,
  period: Period,
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
  const pace = cost.projectedUsd === null ? '' : ` On pace for ${formatUsd(cost.projectedUsd)} over ${length}.`
  const partial = cost.coverage === 'partial' ? ' Partial: use without a price is left out.' : ''
  return `${over.charAt(0).toUpperCase()}${over.slice(1)}.${pace}${partial}`
}

function dayCount(days: number): string {
  return days === 1 ? '1 day' : `${days} days`
}

/**
 * Dollars and cents, without the cents from $100 up or on whole dollars, and
 * `<$0.01` for an amount too small to show in cents.
 */
export function formatUsd(usd: number): string {
  if (usd > 0 && usd < 0.005) {
    return '<$0.01'
  }
  const digits = usd >= 100 || Number.isInteger(usd) ? 0 : 2
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    maximumFractionDigits: digits,
    minimumFractionDigits: digits,
  }).format(usd)
}

/**
 * A share as a whole percent, with a share too small to round up as `<1%`.
 */
export function formatShare(share: number): string {
  const percent = Math.round(share * 100)
  return percent === 0 && share > 0 ? '<1%' : `${percent}%`
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
