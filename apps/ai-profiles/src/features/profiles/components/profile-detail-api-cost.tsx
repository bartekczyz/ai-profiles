import type { FormEvent } from 'react'
import type { AppId } from '@/lib/app-registry'
import type { ApiCost } from '@/lib/types'

import { Suspense, useState } from 'react'

import { format, parseISO } from 'date-fns'
import { ChevronRight } from 'lucide-react'

import { Button, Dialog, Kbd } from '@/design'
import { Input } from '@/design/ui/input'
import { appSpecs } from '@/lib/app-registry'
import { useAppState } from '@/lib/app-state/use-app-state'

import { useProfileApiCost } from '../api/use-profile-api-cost'
import { shortenHomePath } from './shorten-home-path'

/** The days the backend covers. Mirrors `WINDOW_DAYS` in `usage/api_cost.rs`. */
const windowDays = 30

/** Bar colours of the models, most expensive first. Past the last, it repeats. */
const modelColors = ['bg-profile-orange', 'bg-profile-steel', 'bg-profile-sage', 'bg-profile-mauve', 'bg-profile-amber']

/** How many projects the breakdown names before summing up the rest. */
const shownProjects = 8

const eyebrowClasses = 'mb-2 font-mono text-[11.5px] font-medium uppercase tracking-[0.08em] text-muted'

type Props = {
  app: AppId
  profileId: string
}

/**
 * One line under the usage card saying what the profile's Claude Code use
 * would have cost at Claude API prices, which opens the breakdown. Nothing
 * for an app without transcripts to price, or a profile without any yet.
 */
export function ProfileDetailApiCost({ app, profileId }: Props) {
  const enabled = appSpecs[app].hasApiCost
  const { data: cost } = useProfileApiCost(profileId, enabled)
  if (!enabled || !cost || cost.coveredDays === 0) {
    return null
  }
  // Its own boundary, so the pane never waits on the app state the plan
  // price is kept in.
  return (
    <Suspense fallback={null}>
      <ApiCostRow cost={cost} profileId={profileId} />
    </Suspense>
  )
}

function ApiCostRow({ cost, profileId }: { cost: ApiCost; profileId: string }) {
  const { state } = useAppState()
  const [open, setOpen] = useState(false)
  const customCents = state.planPrices[profileId] ?? null
  const planCents = customCents ?? cost.planListPriceCents
  const multiple = planMultiple(cost, planCents)

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="mb-3.5 flex w-full cursor-pointer items-center gap-2 rounded-[10px] border border-border-soft bg-white/30 px-[13px] py-[9px] text-left text-meta transition-colors hover:bg-white/60 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring dark:bg-white/[0.02] dark:hover:bg-white/[0.05]"
      >
        <span className="font-mono text-eyebrow font-medium uppercase tracking-[0.1em] text-muted-strong">
          API equivalent
        </span>
        <span className="font-mono text-mono text-muted-strong">last {cost.coveredDays}d</span>
        <span className="font-medium text-ink tabular-nums">{formatUsd(cost.totalUsd)}</span>
        {multiple !== null ? (
          <span className="truncate text-muted">
            · {cost.projectedUsd !== null ? 'on pace for ' : null}
            {formatMultiple(multiple)} {planPhrase(cost.plan)}
          </span>
        ) : null}
        <ChevronRight aria-hidden size={14} className="ml-auto shrink-0 text-muted-strong" />
      </button>
      <Dialog
        open={open}
        title="API equivalent"
        description={`What the last ${windowDays} days of Claude Code on this profile would have cost at Claude API prices.`}
        className="w-[min(600px,calc(100%-64px))]"
        onClose={() => setOpen(false)}
        foot={
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} onClick={() => setOpen(false)}>
            Close
          </Button>
        }
      >
        <Summary cost={cost} multiple={multiple} />
        <PlanPrice
          profileId={profileId}
          plan={cost.plan}
          listCents={cost.planListPriceCents}
          customCents={customCents}
        />
        <DailyChart cost={cost} />
        <KindBreakdown byKind={cost.byKind} />
        <Projects projects={cost.projects} />
        <Footnote cost={cost} />
      </Dialog>
    </>
  )
}

function Summary({ cost, multiple }: { cost: ApiCost; multiple: number | null }) {
  return (
    <section className="mb-4">
      <p className="m-0 text-[28px] font-semibold tracking-[-0.02em] text-ink tabular-nums">
        {formatUsd(cost.totalUsd)}
      </p>
      <p className="m-0 text-meta text-muted">{summaryText(cost, multiple)}</p>
    </section>
  )
}

type PlanPriceProps = {
  profileId: string
  plan: string | null
  listCents: number | null
  customCents: number | null
}

/**
 * What the plan costs a month, which the multiple compares against: the
 * plan's list price, or what the user said they pay instead (an annual plan,
 * local currency, tax), kept per profile.
 */
function PlanPrice({ profileId, plan, listCents, customCents }: PlanPriceProps) {
  const cents = customCents ?? listCents
  const [editing, setEditing] = useState(false)
  if (editing) {
    return (
      <PlanPriceForm
        profileId={profileId}
        cents={cents}
        canReset={customCents !== null && listCents !== null}
        onDone={() => setEditing(false)}
      />
    )
  }
  return (
    <div className="mb-5 flex items-center gap-2 text-meta text-muted">
      <span>{planPriceLabel(plan, cents, customCents !== null)}</span>
      <Button variant="ghost" size="sm" onClick={() => setEditing(true)}>
        {cents === null ? 'Add price' : 'Edit'}
      </Button>
    </div>
  )
}

type PlanPriceFormProps = {
  profileId: string
  /** The price the form starts from. */
  cents: number | null
  /** Whether the user's price can be dropped for the list price. */
  canReset: boolean
  onDone: () => void
}

function PlanPriceForm({ profileId, cents, canReset, onDone }: PlanPriceFormProps) {
  const { update } = useAppState()
  const [draft, setDraft] = useState(cents === null ? '' : String(cents / 100))
  const [saving, setSaving] = useState(false)
  const draftCents = parseUsdToCents(draft)

  async function save(next: number | null) {
    setSaving(true)
    try {
      await update({ planPrice: { profileId, cents: next } })
      onDone()
    } finally {
      setSaving(false)
    }
  }

  function submit(event: FormEvent) {
    event.preventDefault()
    if (draftCents !== null) {
      void save(draftCents)
    }
  }

  return (
    <form onSubmit={submit} className="mb-5 flex items-center gap-2">
      <label htmlFor="plan-price" className="shrink-0 text-meta text-muted">
        You pay a month, USD
      </label>
      <Input
        autoFocus
        id="plan-price"
        inputMode="decimal"
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        placeholder="20"
        autoComplete="off"
        className="w-24 py-1.5"
      />
      <Button type="submit" variant="primary" size="sm" disabled={draftCents === null || saving}>
        Save
      </Button>
      {canReset ? (
        <Button type="button" variant="ghost" size="sm" disabled={saving} onClick={() => save(null)}>
          Use list price
        </Button>
      ) : null}
      <Button type="button" variant="ghost" size="sm" disabled={saving} onClick={onDone}>
        Cancel
      </Button>
    </form>
  )
}

/** What the plan costs a month, as the breakdown names it. */
function planPriceLabel(plan: string | null, cents: number | null, custom: boolean): string {
  if (cents === null) {
    return 'Add what you pay a month to compare.'
  }
  const name = plan ? `${plan} plan` : 'Your plan'
  return `${name}: ${formatUsd(cents / 100)}/mo${custom ? ', as you set it' : ''}`
}

/**
 * The cost of each day the transcripts cover: the days before the oldest one
 * are left out rather than drawn as days that cost nothing.
 */
function DailyChart({ cost }: { cost: ApiCost }) {
  const days = cost.days.slice(-cost.coveredDays)
  const highest = Math.max(0, ...days.map((day) => day.totalUsd))
  const colorOf = (model: string) =>
    modelColors[
      Math.min(
        cost.byModel.findIndex((each) => each.model === model),
        modelColors.length - 1,
      )
    ]
  const first = days[0]
  return (
    <section className="mb-5">
      <div className="flex items-baseline justify-between">
        <h3 className={eyebrowClasses}>By day</h3>
        <span className="font-mono text-mono text-muted-strong">peak {formatUsd(highest)}/day</span>
      </div>
      <div
        role="img"
        aria-label={`Cost per day over the last ${dayCount(days.length)}`}
        className="flex h-28 items-end gap-[3px] border-b border-border-soft"
      >
        {days.map((day) => (
          <div
            key={day.date}
            title={`${formatDay(day.date)}: ${formatUsd(day.totalUsd)}`}
            className="flex h-full flex-1 flex-col justify-end"
          >
            <div
              className="flex flex-col-reverse overflow-hidden rounded-t-[2px]"
              style={{ height: `${highest > 0 ? (day.totalUsd / highest) * 100 : 0}%` }}
            >
              {day.byModel.map((model) => (
                <div
                  key={model.model}
                  className={colorOf(model.model)}
                  style={{ height: `${(model.usd / day.totalUsd) * 100}%` }}
                />
              ))}
            </div>
          </div>
        ))}
      </div>
      <div className="mt-1 flex justify-between font-mono text-mono text-muted-strong">
        <span>{first ? formatDay(first.date) : null}</span>
        <span>Today</span>
      </div>
      <ul className="m-0 mt-2.5 flex list-none flex-wrap gap-x-4 gap-y-1 p-0 text-meta text-muted">
        {cost.byModel.map((model) => (
          <li key={model.model} className="flex items-center gap-1.5">
            <span aria-hidden className={`h-2 w-2 rounded-[2px] ${colorOf(model.model)}`} />
            {model.model}
            <span className="text-ink tabular-nums">{formatUsd(model.usd)}</span>
          </li>
        ))}
      </ul>
    </section>
  )
}

function KindBreakdown({ byKind }: { byKind: ApiCost['byKind'] }) {
  const rows = kindRows(byKind)
  const total = rows.reduce((sum, row) => sum + row.usd, 0)
  return (
    <section className="mb-5">
      <h3 className={eyebrowClasses}>By what was billed</h3>
      <CostRows rows={rows.map((row) => ({ ...row, share: total > 0 ? row.usd / total : 0 }))} />
    </section>
  )
}

function Projects({ projects }: { projects: ApiCost['projects'] }) {
  const shown = projects.slice(0, shownProjects)
  const rest = projects.slice(shownProjects)
  const rows = shown.map((project) => ({
    label: project.path === null ? 'Unknown folder' : folderName(project.path),
    title: project.path === null ? undefined : shortenHomePath(project.path),
    usd: project.usd,
  }))
  if (rest.length > 0) {
    rows.push({
      label: `${rest.length} more`,
      title: undefined,
      usd: rest.reduce((sum, project) => sum + project.usd, 0),
    })
  }
  return (
    <section className="mb-5">
      <h3 className={eyebrowClasses}>By project</h3>
      <CostRows rows={rows} />
    </section>
  )
}

type CostRow = { label: string; title?: string; usd: number; share?: number }

function CostRows({ rows }: { rows: Array<CostRow> }) {
  return (
    <ul className="m-0 list-none p-0 text-meta">
      {rows.map((row) => (
        <li key={row.label} className="flex items-baseline gap-2 border-b border-border-soft py-1.5 last:border-b-0">
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

function Footnote({ cost }: { cost: ApiCost }) {
  return (
    <section className="text-meta text-muted">
      {cost.unpriced.length > 0 ? <p className="m-0 mb-2 text-amber">{unpricedNote(cost.unpriced)}</p> : null}
      <p className="m-0">
        From this profile's Claude Code transcripts, at Claude API list prices as of{' '}
        {format(parseISO(cost.pricesAsOf), 'MMM d, yyyy')}. Chats in the desktop app and on claude.ai keep no token
        counts, so they aren't included.
      </p>
    </section>
  )
}

/**
 * How many times what the plan costs a month the use comes to over the
 * window: at the pace so far when the transcripts cover only part of it.
 * `null` without a price to compare against.
 */
export function planMultiple(
  cost: Pick<ApiCost, 'totalUsd' | 'projectedUsd'>,
  planCents: number | null,
): number | null {
  if (planCents === null || planCents <= 0) {
    return null
  }
  return (cost.projectedUsd ?? cost.totalUsd) / (planCents / 100)
}

export function formatMultiple(multiple: number): string {
  return multiple >= 10 ? `${Math.round(multiple)}×` : `${multiple.toFixed(1)}×`
}

/** The line under the total: the days it covers and how it compares. */
export function summaryText(cost: Pick<ApiCost, 'coveredDays' | 'projectedUsd'>, multiple: number | null): string {
  const pays = multiple === null ? '' : `, ${formatMultiple(multiple)} what you pay`
  if (cost.projectedUsd === null) {
    return `Over the last ${windowDays} days${pays}.`
  }
  return `Over the ${dayCount(cost.coveredDays)} your transcripts cover. On pace for ${formatUsd(cost.projectedUsd)} over ${windowDays} days${pays}.`
}

function dayCount(days: number): string {
  return days === 1 ? '1 day' : `${days} days`
}

function planPhrase(plan: string | null): string {
  return plan ? `your ${plan} plan` : 'your plan'
}

/** Dollars and cents, without the cents from $100 up or on whole dollars. */
export function formatUsd(usd: number): string {
  const digits = usd >= 100 || Number.isInteger(usd) ? 0 : 2
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    maximumFractionDigits: digits,
    minimumFractionDigits: digits,
  }).format(usd)
}

/** A share as a whole percent, with a share too small to round up as `<1%`. */
export function formatShare(share: number): string {
  const percent = Math.round(share * 100)
  return percent === 0 && share > 0 ? '<1%' : `${percent}%`
}

function formatDay(date: string): string {
  return format(parseISO(date), 'MMM d')
}

/** The last part of `path`, a folder, or `~` for the home folder. */
function folderName(path: string): string {
  const short = shortenHomePath(path)
  return short === '~' ? short : (path.split('/').filter(Boolean).pop() ?? path)
}

/**
 * A price typed in dollars, `$` or not, in cents: `null` for anything that
 * isn't a positive amount.
 */
export function parseUsdToCents(text: string): number | null {
  const trimmed = text.trim().replace(/^\$/, '').trim()
  if (!/^\d+(\.\d{0,2})?$/.test(trimmed)) {
    return null
  }
  const cents = Math.round(Number(trimmed) * 100)
  return cents > 0 ? cents : null
}

/** The billed kinds with any cost, most expensive first. */
export function kindRows(byKind: ApiCost['byKind']): Array<{ label: string; usd: number }> {
  const rows = [
    { label: 'Input', usd: byKind.input },
    { label: 'Output', usd: byKind.output },
    { label: 'Cache writes, 5 min', usd: byKind.cacheWrite5m },
    { label: 'Cache writes, 1 hour', usd: byKind.cacheWrite1h },
    { label: 'Cache reads', usd: byKind.cacheRead },
    { label: 'Web search', usd: byKind.webSearch },
  ]
  return rows.filter((row) => row.usd > 0).sort((left, right) => right.usd - left.usd)
}

export function unpricedNote(unpriced: ApiCost['unpriced']): string {
  const models = unpriced.map((model) => model.model).join(', ')
  const tokens = unpriced.reduce((sum, model) => sum + model.tokens, 0)
  return `No price for ${models} (${tokens.toLocaleString('en-US')} tokens), so they aren't counted. A newer ai-profiles may know it.`
}
