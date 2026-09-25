import type { ReactNode } from 'react'
import type { AppId } from '@/lib/app-registry'
import type { ProfileUsage, QuotaError, RateLimitResetCredits, Spend, UsageWindow } from '@/lib/types'

import { Component, createContext, useContext, useEffect, useState } from 'react'

import { format } from 'date-fns'
import { ArrowLeftRight, RefreshCw } from 'lucide-react'

import { Skeleton, TooltipBubble } from '@/design'
import { appSpecs } from '@/lib/app-registry'
import { openCliLogin } from '@/lib/commands'

import { refetchIntervalMs, UsageUnavailableError, useProfileUsage } from '../api/use-profile-usage'

/**
 * The usage block's container: the same inset grouped panel as the surfaces
 * panel below it — same radius, same 13px gutter — so the pane has one
 * container treatment rather than a card here and a panel there.
 *
 * Deliberately a weight lighter than the surfaces panel (soft hairline,
 * thinner wash, no internal row dividers). Usage is the one block whose
 * height moves between quota states — two meters, three, a skeleton, an
 * error line, a recovery button — and a lighter container makes that
 * movement far less disruptive than locking the height would be.
 *
 * The bottom margin matches the gap the prototype puts between panels; the
 * card owns it because it renders nothing at all for a profile without a
 * CLI surface, and a caller-supplied gap would then hang in empty space.
 */
const usagePanelClasses =
  'mb-3.5 rounded-[10px] border border-border-soft bg-white/30 px-[13px] py-[9px] dark:bg-white/[0.02]'

type Props = {
  app: AppId
  profileId: string
  cliEnabled: boolean
  /** Exact CLI command for this profile (`claude-<slug>` wrapper for managed
   * profiles). Falls back to the stock binary name for the default entry. */
  cliCommand?: string
}

export function ProfileDetailUsageCard({ app, profileId, cliEnabled, cliCommand }: Props) {
  // Bumped by the in-boundary Retry button to force the inner query
  // to re-run after a render-time crash. We use it (alongside profileId)
  // as the key on the boundary itself, so switching profiles or hitting
  // Retry remounts the boundary — its hasError state resets along with
  // the inner useQuery's cache subscription.
  const [attempt, setAttempt] = useState(0)
  if (!cliEnabled || !appSpecs[app].hasUsage) {
    return null
  }
  return (
    <UsageCardErrorBoundary key={`${profileId}:${attempt}`} onRetry={() => setAttempt((value) => value + 1)}>
      <UsageCardInner app={app} cliCommand={cliCommand ?? appSpecs[app].cliBinary} profileId={profileId} />
    </UsageCardErrorBoundary>
  )
}

export type UsageDisplay = 'used' | 'remaining'
const usageDisplayKey = 'ai-profiles-codex-usage-display'
const UsageDisplayContext = createContext<UsageDisplay>('used')

function readUsageDisplay(): UsageDisplay {
  try {
    return window.localStorage.getItem(usageDisplayKey) === 'remaining' ? 'remaining' : 'used'
  } catch {
    return 'used'
  }
}

/**
 * Flips between the two usage displays. Kept as its own function so the
 * toggle's target value can't drift from the copy below it describes.
 */
export function toggleUsageDisplay(display: UsageDisplay): UsageDisplay {
  return display === 'used' ? 'remaining' : 'used'
}

export type CodexDisplayToggleCopy = {
  /**
   * Text shown on the toggle button itself, naming the CURRENT display.
   */
  buttonLabel: string
  /**
   * Accessible name for the toggle button, describing the action it performs.
   */
  ariaLabel: string
  /**
   * Tooltip text shown on hover, describing the action it performs.
   */
  tooltip: string
}

/**
 * Copy for the Codex used/remaining toggle button, derived from which
 * display is currently shown. One function keeps the button text,
 * aria-label, and tooltip from drifting out of sync with each other.
 */
export function codexDisplayToggleCopy(display: UsageDisplay): CodexDisplayToggleCopy {
  if (display === 'used') {
    return { buttonLabel: 'Used', ariaLabel: 'Show remaining quota', tooltip: 'Switch to remaining quota' }
  }
  return { buttonLabel: 'Remaining', ariaLabel: 'Show used quota', tooltip: 'Switch to used quota' }
}

function UsageCardInner({ app, profileId, cliCommand }: { app: AppId; profileId: string; cliCommand: string }) {
  const { data, error, isLoading, isFetching, dataUpdatedAt, refetch } = useProfileUsage(profileId)
  const errorCode = usageErrorCode(error)
  const [display, setDisplay] = useState<UsageDisplay>(readUsageDisplay)
  function changeDisplay(next: UsageDisplay) {
    setDisplay(next)
    try {
      window.localStorage.setItem(usageDisplayKey, next)
    } catch {
      // Storage may be unavailable; switching still works for this session.
    }
  }
  const toggleCopy = codexDisplayToggleCopy(display)

  return (
    <UsageDisplayContext.Provider value={app === 'codex' ? display : 'used'}>
      <section className={usagePanelClasses}>
        <header className="mb-1.5 flex min-h-[22px] items-center justify-between gap-3.5">
          <div className="flex min-w-0 items-center gap-1 font-mono text-muted-strong">
            <span className="text-eyebrow font-medium uppercase tracking-[0.1em]">Usage</span>
            {app === 'codex' ? (
              <>
                <span aria-hidden className="text-mono">
                  ·
                </span>
                <button
                  type="button"
                  aria-label={toggleCopy.ariaLabel}
                  onClick={() => changeDisplay(toggleUsageDisplay(display))}
                  className="group relative inline-flex min-h-[22px] cursor-pointer items-center gap-1 rounded px-1 text-mono transition-colors hover:bg-ink/[0.06] hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
                >
                  {toggleCopy.buttonLabel}
                  <ArrowLeftRight aria-hidden size={10} className="opacity-60" />
                  <TooltipBubble>{toggleCopy.tooltip}</TooltipBubble>
                </button>
              </>
            ) : null}
          </div>
          <div className="flex items-center gap-[7px]">
            <RefreshCountdown isFetching={isFetching} dataUpdatedAt={dataUpdatedAt} />
            <button
              type="button"
              aria-label="Refresh usage"
              disabled={isFetching}
              onClick={() => refetch()}
              className="grid h-[22px] w-[22px] cursor-pointer place-items-center rounded-full text-muted-strong transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:not-disabled:bg-ink/[0.06] hover:not-disabled:text-ink disabled:cursor-default disabled:opacity-50"
            >
              <RefreshCw size={14} className={isFetching ? 'animate-spin' : undefined} />
            </button>
          </div>
        </header>

        {isLoading ? (
          <MetersSkeleton />
        ) : (
          <Body
            app={app}
            cliCommand={cliCommand}
            errorCode={errorCode}
            profileId={profileId}
            quota={data?.quota ?? null}
          />
        )}
      </section>
    </UsageDisplayContext.Provider>
  )
}

function RefreshCountdown({ isFetching, dataUpdatedAt }: { isFetching: boolean; dataUpdatedAt: number | undefined }) {
  // Force re-render every 5s so the countdown ticks down without us
  // wiring an explicit timer per second. 5s is plenty since the label
  // resolution is minutes for most of the window.
  const [, setTick] = useState(0)
  useEffect(() => {
    const id = setInterval(() => setTick((value) => value + 1), 5_000)
    return () => clearInterval(id)
  }, [])

  if (isFetching) {
    return <span className="font-mono text-mono text-muted-strong">refreshing…</span>
  }
  if (!dataUpdatedAt) {
    return null
  }
  // Fresh data shows a countdown to the next auto-refresh; data older than the
  // refresh interval (e.g. a snapshot restored from a previous session) shows
  // its age instead, so the staleness is visible at a glance.
  const ageMs = Date.now() - dataUpdatedAt
  if (ageMs >= refetchIntervalMs) {
    return <span className="font-mono text-mono text-muted-strong">updated {formatUpdatedAgo(ageMs)}</span>
  }
  const label = formatRefreshIn(dataUpdatedAt + refetchIntervalMs - Date.now())
  if (!label) {
    return null
  }
  return <span className="font-mono text-mono text-muted-strong">refresh in {label}</span>
}

function formatRefreshIn(deltaMs: number): string | null {
  if (deltaMs <= 0) {
    return 'soon'
  }
  const totalSeconds = Math.floor(deltaMs / 1000)
  if (totalSeconds >= 60) {
    return `${Math.floor(totalSeconds / 60)}m`
  }
  return `${totalSeconds}s`
}

function formatUpdatedAgo(ageMs: number): string {
  const minutes = Math.floor(ageMs / 60_000)
  if (minutes < 1) {
    return 'just now'
  }
  if (minutes < 60) {
    return `${minutes}m ago`
  }
  const hours = Math.floor(minutes / 60)
  if (hours < 24) {
    return `${hours}h ago`
  }
  return `${Math.floor(hours / 24)}d ago`
}

// Maps a thrown query error to a quota error code, or null when there's no
// error. The query only ever throws `UsageUnavailableError`; anything else
// is unexpected and maps to the neutral `unknown` message.
function usageErrorCode(error: unknown): QuotaError | null {
  if (error instanceof UsageUnavailableError) {
    return error.code
  }
  if (error) {
    return 'unknown'
  }
  return null
}

function Body({
  app,
  quota,
  errorCode,
  cliCommand,
  profileId,
}: {
  app: AppId
  quota: ProfileUsage['quota']
  errorCode: QuotaError | null
  cliCommand: string
  profileId: string
}) {
  // Stale-while-revalidate: whenever there's data, show the meters — even if
  // the latest refresh just failed — with a quiet "couldn't refresh" note so
  // the staleness stays honest. The full error message is reserved for when
  // there's nothing cached to show.
  if (quota) {
    return (
      <div className="flex flex-col gap-2">
        <Meters app={app} quota={quota} />
        {errorCode ? (
          <p className="font-mono text-mono text-muted-strong">Couldn't refresh — {quotaErrorShort(errorCode)}.</p>
        ) : null}
        {canRelogin(errorCode) ? <ReloginButton profileId={profileId} /> : null}
      </div>
    )
  }
  if (errorCode) {
    return (
      <div className="flex flex-col gap-2">
        <p className="font-mono text-mono text-muted-strong">{quotaErrorMessage(app, errorCode, cliCommand)}</p>
        {canRelogin(errorCode) ? <ReloginButton profileId={profileId} /> : null}
      </div>
    )
  }
  return <MetersSkeleton />
}

// A failed token refresh and an expired session both recover the same way:
// run the profile's CLI interactively once. The button opens it in Terminal.
function canRelogin(errorCode: QuotaError | null): boolean {
  return errorCode === 'needs_login' || errorCode === 'unauthorized'
}

function ReloginButton({ profileId }: { profileId: string }) {
  return (
    <button
      type="button"
      onClick={() => {
        void openCliLogin(profileId)
      }}
      className="cursor-pointer self-start font-mono text-mono text-muted-strong underline hover:text-ink"
    >
      Refresh sign-in
    </button>
  )
}

// Terse reason appended to the "Couldn't refresh — …" note shown beside stale
// meters. The full sentences in `quotaErrorMessage` are for the no-data case.
function quotaErrorShort(quotaError: QuotaError): string {
  if (quotaError === 'no_credentials') {
    return 'sign-in needed'
  }
  if (quotaError === 'needs_login') {
    return 'sign-in needed'
  }
  if (quotaError === 'unauthorized') {
    return 'token refresh needed'
  }
  if (quotaError === 'forbidden') {
    return 'blocked upstream'
  }
  if (quotaError === 'rate_limited') {
    return 'rate limited'
  }
  if (quotaError === 'network') {
    return 'offline'
  }
  return 'unavailable'
}

// Resolves the message shown in place of the meters for a given error code.
// All app-specific copy lives in the registry so a ChatGPT pane never names
// Anthropic (and vice versa); unknown stays neutral.
export function quotaErrorMessage(app: AppId, quotaError: QuotaError, cliCommand: string): string {
  const usage = appSpecs[app].usage
  if (quotaError === 'no_credentials') {
    return usage?.noCredentials ?? 'Sign in once with this profile to see usage.'
  }
  if (quotaError === 'needs_login') {
    return `Session expired — run \`${cliCommand}\` and sign in to this profile again.`
  }
  if (quotaError === 'unauthorized') {
    // Not a real "session expired" — the CLI's short-lived access token rolls
    // over and is refreshed the next time you invoke it interactively.
    return `Token refresh needed — run \`${cliCommand}\` once, then retry.`
  }
  if (quotaError === 'forbidden') {
    return 'Usage request was blocked upstream — usually transient. Try again later.'
  }
  if (quotaError === 'rate_limited') {
    return usage?.rateLimited ?? 'Rate limited. Try again in a few minutes.'
  }
  if (quotaError === 'network') {
    return usage?.networkError ?? "Couldn't reach the usage service — check your connection and retry."
  }
  return "Couldn't load usage stats. Try again."
}

function CreditExpiry({ expiresAt }: { expiresAt: number | null }) {
  if (expiresAt == null) return <>Expiry unavailable</>
  const date = new Date(expiresAt * 1000)
  const deltaMs = date.getTime() - Date.now()
  return (
    <span className="group relative inline-block">
      {deltaMs <= 0 ? 'expired' : formatResetRelative(deltaMs, 'expires')}
      <TooltipBubble>{format(date, 'EEE d MMM yyyy, HH:mm')}</TooltipBubble>
    </span>
  )
}

function AvailableResets({ resets }: { resets: RateLimitResetCredits | undefined }) {
  if (!resets || resets.availableCount === 0) return null
  const count = resets.availableCount
  const credits = (resets.credits ?? []).filter((credit) => credit.status === 'available').slice(0, count)
  const missing = count - credits.length
  return (
    <div className="mt-1 border-t border-border-soft pt-2 font-mono text-mono text-muted-strong">
      <p>{`${count} reset${count === 1 ? '' : 's'} available`}</p>
      {credits.map((credit, index) => (
        // Credit IDs are deliberately omitted from the display-only payload.
        // biome-ignore lint/suspicious/noArrayIndexKey: immutable snapshot rows have no client state
        <p key={index} className="mt-1">
          {credit.title || 'Quota reset'} · <CreditExpiry expiresAt={credit.expiresAt} />
        </p>
      ))}
      {missing > 0 ? (
        <p className="mt-1">
          Expiry details unavailable for {missing} reset{missing === 1 ? '' : 's'}.
        </p>
      ) : null}
    </div>
  )
}

export type CodexWindowLabel = {
  /**
   * Full label shown at wide viewports, e.g. "5-hour window".
   */
  label: string
  /**
   * Collapsed label shown at narrow viewports, e.g. "5h".
   */
  shortLabel: string
}

/**
 * Labels for a Codex primary/secondary meter, derived from its window
 * duration. Codex windows are positions, not fixed periods, so the label is
 * computed from whatever duration the payload reports rather than hardcoded
 * per slot.
 */
export function codexWindowLabel(minutes: number | null | undefined): CodexWindowLabel {
  if (minutes === 10080) {
    return { label: 'Weekly', shortLabel: 'W' }
  }
  if (minutes == null) {
    return { label: 'Usage window', shortLabel: 'Usage' }
  }
  if (minutes % 60 === 0) {
    return { label: `${minutes / 60}-hour window`, shortLabel: `${minutes / 60}h` }
  }
  return { label: `${minutes}-minute window`, shortLabel: `${minutes}m` }
}

export type CodexMeterRow = {
  /**
   * Which Codex quota position the row renders.
   */
  slot: 'primary' | 'secondary'
  /**
   * The window's usage data.
   */
  window: UsageWindow
  /**
   * Full label shown at wide viewports.
   */
  label: string
  /**
   * Collapsed label shown at narrow viewports.
   */
  shortLabel: string
  /**
   * Whether the bar is split into daily segments (weekly windows only).
   */
  showDailySegments: boolean
  /**
   * Window length used to compute the pace marker; absent when unknown.
   */
  paceWindowMins: number | null | undefined
}

/**
 * The Codex primary/secondary rows to render, in slot order, skipping any
 * slot the payload left empty. Codex primary/secondary are positions, not
 * fixed time periods — a weekly-only plan can put its weekly quota in
 * primary — so an absent window is never invented.
 */
export function codexMeterRows(quota: ProfileUsage['quota']): Array<CodexMeterRow> {
  const rows: Array<CodexMeterRow> = []
  for (const slot of ['primary', 'secondary'] as const) {
    const window = quota?.[slot]
    if (!window) {
      continue
    }
    const minutes = window.windowDurationMins
    const { label, shortLabel } = codexWindowLabel(minutes)
    rows.push({
      slot,
      window,
      label,
      shortLabel,
      showDailySegments: minutes === 10080,
      paceWindowMins: minutes,
    })
  }
  return rows
}

/**
 * Scoped-weekly rows to render for an app that reports per-model weekly
 * sub-quotas (Claude). Skips a row the user hasn't touched this window
 * (utilization explicitly 0) so the card stays focused. Unknown utilization
 * (null) is kept visible — we'd rather show a placeholder than silently drop
 * a window we lack data for.
 */
export function visibleScopedWeekly(
  hasScopedWeekly: boolean | undefined,
  scopedWeekly: Array<UsageWindow> | undefined,
): Array<UsageWindow> {
  if (!hasScopedWeekly) {
    return []
  }
  return (scopedWeekly ?? []).filter((window) => window.utilization !== 0)
}

export function Meters({ app, quota }: { app: AppId; quota: ProfileUsage['quota'] }) {
  if (app === 'codex') {
    return (
      <div className="flex flex-col gap-2">
        {codexMeterRows(quota).map((row) => (
          <Meter
            key={row.slot}
            label={row.label}
            shortLabel={row.shortLabel}
            meterWindow={row.window}
            showDailySegments={row.showDailySegments}
            paceWindowMins={row.paceWindowMins}
          />
        ))}
        <AvailableResets resets={quota?.rateLimitResetCredits} />
      </div>
    )
  }
  const usageCopy = appSpecs[app].usage
  const scopedWeekly = visibleScopedWeekly(usageCopy?.hasScopedWeekly, quota?.scopedWeekly)
  return (
    <div className="flex flex-col gap-2">
      <Meter
        label={usageCopy?.primaryLabel ?? '5-hour window'}
        shortLabel={usageCopy?.primaryShortLabel ?? '5h'}
        meterWindow={quota?.primary ?? null}
      />
      <Meter
        showDailySegments
        paceWindowMins={10080}
        label={usageCopy?.secondaryLabel ?? 'Weekly'}
        shortLabel={usageCopy?.secondaryShortLabel ?? 'W'}
        meterWindow={quota?.secondary ?? null}
      />
      {scopedWeekly.map((window, index) => (
        <Meter
          showDailySegments
          // Model names are unique within a response; the index only backs
          // up the rare unlabelled row.
          key={window.label ?? `scoped-${index}`}
          paceWindowMins={10080}
          label={scopedWeeklyLabel(window.label)}
          shortLabel={scopedWeeklyShortLabel(window.label)}
          meterWindow={window}
        />
      ))}
      <SpendMeter spend={quota?.spend} />
    </div>
  )
}

// The model a scoped weekly applies to is server-supplied and changes over
// time (Sonnet → Opus → Fable), so the row names it from the payload and
// falls back to a neutral label rather than a stale hardcoded model.
function scopedWeeklyLabel(label: string | null | undefined): string {
  if (!label) {
    return 'Weekly (scoped)'
  }
  return `Weekly · ${label}`
}

// Narrow viewports get a ~32px label column, so the model collapses to its
// initial: "Weekly · Fable" → "WF".
function scopedWeeklyShortLabel(label: string | null | undefined): string {
  if (!label) {
    return 'W*'
  }
  return `W${label.charAt(0).toUpperCase()}`
}

/**
 * Pay-as-you-go credit spend, rendered as a meter alongside the quota
 * windows. A cap is what makes the bar meaningful, so an uncapped account
 * gets no row at all rather than a bar with nothing to fill against.
 */
function SpendMeter({ spend }: { spend: Spend | undefined }) {
  if (!spend || spend.limitMinor === null) {
    return null
  }
  const percent = spend.percent ?? (spend.usedMinor / spend.limitMinor) * 100
  const used = formatMoney(spend.usedMinor, spend.currency, spend.exponent)
  const limit = formatMoney(spend.limitMinor, spend.currency, spend.exponent)
  return (
    <Meter
      label="Usage credits"
      shortLabel="Cr"
      meterWindow={{ utilization: percent, resetsAt: null }}
      trailing={`${used} of ${limit}`}
    />
  )
}

// Minor units → a localised currency string. The amount is only divided
// down at the last moment so the integer the backend sent stays exact.
function formatMoney(amountMinor: number, currency: string, exponent: number): string {
  const amount = amountMinor / 10 ** exponent
  try {
    return new Intl.NumberFormat(undefined, {
      style: 'currency',
      currency,
      minimumFractionDigits: exponent,
      maximumFractionDigits: exponent,
    }).format(amount)
  } catch {
    // Intl throws on a currency code it doesn't know. A bare number with
    // the code beside it is still more use than an empty row.
    return `${amount.toFixed(exponent)} ${currency}`
  }
}

// Layout: [label] [bar (1fr)] [trailing text fixed width]. The fixed
// trailing column keeps every bar exactly the same width across rows
// and reserves space for the longest "100% · resets in 23h 59m"
// string (~22 mono chars ≈ 180px). The label column shrinks at narrow
// viewports so the bar still has room to breathe.
//
// The minimum height is what the mono label occupies naturally; stating it
// lets the loading placeholder hit the same row height without having to
// carry text of its own, so the meters don't nudge downward on arrival.
const meterGridClass =
  'grid min-h-[15px] grid-cols-[32px_1fr_180px] items-center gap-2 lg:grid-cols-[140px_1fr_180px] lg:gap-3'

/**
 * Severity level for a meter's bar, driving both its fill color and (via
 * `meterToneBarClass`) the Tailwind class that paints it.
 */
export type MeterTone = 'muted' | 'ok' | 'warn' | 'crit'

/**
 * Rounds raw utilization (0..=100+, uncapped when a user is over-limit) into
 * the whole-percent value the label and tone lookup both key off. Null
 * utilization (no data yet) stays null rather than becoming a false 0%.
 */
export function usedPercentFromUtilization(utilization: number | null): number | null {
  if (utilization === null) {
    return null
  }
  return Math.round(utilization)
}

/**
 * Flips a used-percent into whichever display the user has toggled to.
 * "used" passes the value through; "remaining" inverts it and floors at 0
 * so an over-limit account (over 100% used) never shows negative remaining.
 */
export function displayPercent(usedPercent: number | null, display: UsageDisplay): number | null {
  if (usedPercent === null) {
    return null
  }
  if (display === 'remaining') {
    return Math.max(0, 100 - usedPercent)
  }
  return usedPercent
}

/**
 * Clamps a display percent into the 0..100 range a bar's width can render.
 * Null (no data) fills to 0 — an empty bar rather than a full one.
 */
export function meterFillPercent(percent: number | null): number {
  if (percent === null) {
    return 0
  }
  return Math.min(100, Math.max(0, percent))
}

/**
 * Severity tone for a meter's bar, derived from used-percent regardless of
 * which display the user is viewing — tone always tracks how much of the
 * quota is actually used, never the remaining view's flipped number.
 */
export function meterTone(usedPercent: number | null): MeterTone {
  if (usedPercent === null) {
    return 'muted'
  }
  if (usedPercent < 50) {
    return 'ok'
  }
  if (usedPercent < 80) {
    return 'warn'
  }
  return 'crit'
}

const meterToneBarClass: Record<MeterTone, string> = {
  muted: 'bg-muted-strong',
  ok: 'bg-green',
  warn: 'bg-amber',
  crit: 'bg-red',
}

/**
 * Flips a pace-marker position into whichever display the user has toggled
 * to, mirroring `displayPercent` — "remaining" mirrors the marker across
 * the bar rather than recomputing it from a remaining-based percent.
 */
export function displayPacePercent(pacePercent: number | null, display: UsageDisplay): number | null {
  if (pacePercent === null) {
    return null
  }
  if (display === 'remaining') {
    return 100 - pacePercent
  }
  return pacePercent
}

type MeterLabelColumnProps = {
  /**
   * Full label shown at wide viewports.
   */
  label: string
  /**
   * Collapsed label shown at narrow viewports.
   */
  shortLabel: string
}

/**
 * The meter's label column: a short initial at narrow viewports, the full
 * word at wide ones.
 */
function MeterLabelColumn({ label, shortLabel }: MeterLabelColumnProps) {
  return (
    <span className="font-mono text-mono text-muted-strong">
      <span className="lg:hidden">{shortLabel}</span>
      <span className="hidden lg:inline">{label}</span>
    </span>
  )
}

type MeterBarProps = {
  /**
   * Accessible name for the progressbar element.
   */
  ariaLabel: string
  /**
   * Display-adjusted fill percent (0..100+ before clamping), or null when
   * there's no data. Read for the progressbar's `aria-valuenow`/`aria-valuetext`.
   */
  percent: number | null
  /**
   * Which of used/remaining `percent` is expressed in, named in
   * `aria-valuetext` and used to derive the pace marker's side of the bar.
   */
  display: UsageDisplay
  /**
   * Clamped 0..100 percent the bar's fill is drawn at.
   */
  fillPercent: number
  /**
   * Tailwind background class painting the fill, chosen by tone.
   */
  barClass: string
  /**
   * Whether to draw the weekly day separators over the track.
   */
  showDailySegments: boolean
  /**
   * Display-adjusted pace-marker position (0..100), or null to omit the
   * marker entirely.
   */
  pacePercent: number | null
}

/**
 * The meter's progress bar: the filled track, optional weekly day
 * separators, and an optional pace marker positioned in the same display
 * (used/remaining) as the fill itself.
 */
function MeterBar({
  ariaLabel,
  percent,
  display,
  fillPercent,
  barClass,
  showDailySegments,
  pacePercent,
}: MeterBarProps) {
  const remaining = display === 'remaining'
  return (
    <div className="relative">
      <div
        role="progressbar"
        aria-valuenow={percent ?? undefined}
        aria-valuetext={percent === null ? undefined : `${percent}% ${display}`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={ariaLabel}
        className="relative h-1.5 overflow-hidden rounded-full bg-cream-3"
      >
        <div className={`h-full rounded-full ${barClass}`} style={{ width: `${fillPercent}%` }} />
        {showDailySegments ? <DaySeparators /> : null}
      </div>
      {pacePercent === null ? null : <PaceMarker percent={pacePercent} remaining={remaining} />}
    </div>
  )
}

type MeterTrailingProps = {
  /**
   * Display-adjusted percent shown before the reset text, or null to show
   * the placeholder dash.
   */
  percent: number | null
  /**
   * Relative/absolute reset text shown after the percent, or null to omit it.
   */
  resetLabel: ResetLabel | null
  /**
   * Replaces the default "42% · resets in 3h" text entirely. Used by rows
   * measured in something other than a percentage of a time window.
   */
  trailing: ReactNode
}

/**
 * The meter's trailing column: caller-supplied text (e.g. spend), or the
 * default "42% · resets in 3h" built from the percent and reset label.
 */
function MeterTrailing({ trailing, percent, resetLabel }: MeterTrailingProps) {
  return (
    <span className="text-right font-mono text-mono tabular-nums text-muted-strong">
      {trailing ?? (
        <>
          {percent === null ? '—' : `${percent}%`}
          {resetLabel ? (
            <span className="group relative inline-block">
              {` · ${resetLabel.relative}`}
              <TooltipBubble>{resetLabel.absolute}</TooltipBubble>
            </span>
          ) : null}
        </>
      )}
    </span>
  )
}

function Meter({
  label,
  shortLabel,
  meterWindow,
  showDailySegments = false,
  paceWindowMins,
  trailing,
}: {
  label: string
  shortLabel: string
  meterWindow: UsageWindow | null
  showDailySegments?: boolean
  paceWindowMins?: number | null
  /**
   * Replaces the default "42% · resets in 3h" trailing text. Used by rows
   * measured in something other than a percentage of a time window.
   */
  trailing?: ReactNode
}) {
  const display = useContext(UsageDisplayContext)
  const usedPercent = usedPercentFromUtilization(meterWindow?.utilization ?? null)
  const percent = displayPercent(usedPercent, display)
  const fillPercent = meterFillPercent(percent)
  const barClass = meterToneBarClass[meterTone(usedPercent)]
  const resetLabel = formatReset(meterWindow?.resetsAt ?? null)
  const pacePercent = computePacePercent(meterWindow?.resetsAt ?? null, paceWindowMins)

  return (
    <div className={meterGridClass}>
      <MeterLabelColumn label={label} shortLabel={shortLabel} />
      <MeterBar
        showDailySegments={showDailySegments}
        ariaLabel={label}
        barClass={barClass}
        display={display}
        fillPercent={fillPercent}
        pacePercent={displayPacePercent(pacePercent, display)}
        percent={percent}
      />
      <MeterTrailing percent={percent} resetLabel={resetLabel} trailing={trailing} />
    </div>
  )
}

function DaySeparators() {
  return (
    <>
      {[1, 2, 3, 4, 5, 6].map((day) => (
        <div
          key={day}
          aria-hidden
          className="pointer-events-none absolute top-0 h-full w-px bg-ink/10"
          style={{ left: `${(day / 7) * 100}%` }}
        />
      ))}
    </>
  )
}

function PaceMarker({ percent, remaining = false }: { percent: number; remaining?: boolean }) {
  const clamped = Math.min(100, Math.max(0, percent))
  return (
    <div
      className="group absolute -top-0.5 flex h-2.5 w-3 items-center justify-center"
      style={{ left: `calc(${clamped}% - 6px)` }}
    >
      <div aria-hidden className="pointer-events-none h-full w-0.5 rounded-sm bg-ink" />
      <TooltipBubble>
        Even daily pace · {Math.round(clamped)}%{remaining ? ' remaining' : ''}
      </TooltipBubble>
    </div>
  )
}

// Pace uses elapsed time within the actual quota window, independently of
// the weekly day separators. Missing or invalid timing cannot imply a pace.
function computePacePercent(resetsAt: string | null, durationMins: number | null | undefined): number | null {
  if (!resetsAt || durationMins == null || !Number.isFinite(durationMins) || durationMins <= 0) {
    return null
  }
  const resetTime = new Date(resetsAt).getTime()
  if (Number.isNaN(resetTime)) {
    return null
  }
  const windowMs = durationMins * 60 * 1000
  if (!Number.isFinite(windowMs)) return null
  const timeRemaining = resetTime - Date.now()
  if (timeRemaining <= 0) {
    return 100
  }
  if (timeRemaining >= windowMs) {
    return 0
  }
  return ((windowMs - timeRemaining) / windowMs) * 100
}

type ResetLabel = {
  /** Compact relative phrase used inline, e.g. "resets in 23h 59m". */
  relative: string
  /** Absolute datetime shown as the hover tooltip, e.g. "Sat 30 May, 14:30". */
  absolute: string
}

function formatReset(resetsAt: string | null): ResetLabel | null {
  if (!resetsAt) {
    return null
  }
  const date = new Date(resetsAt)
  if (Number.isNaN(date.getTime())) {
    return null
  }
  return {
    relative: formatResetRelative(date.getTime() - Date.now()),
    absolute: format(date, 'EEE d MMM, HH:mm'),
  }
}

function formatResetRelative(deltaMs: number, verb = 'resets'): string {
  if (deltaMs <= 0) {
    return `${verb} soon`
  }
  const hours = Math.floor(deltaMs / (60 * 60 * 1000))
  const minutes = Math.floor((deltaMs % (60 * 60 * 1000)) / (60 * 1000))
  if (hours >= 24) {
    const days = Math.floor(hours / 24)
    return `${verb} in ${days}d`
  }
  if (hours >= 1) {
    return `${verb} in ${hours}h ${minutes}m`
  }
  return `${verb} in ${minutes}m`
}

function MetersSkeleton() {
  return (
    <div className="flex flex-col gap-2">
      {[0, 1, 2].map((row) => (
        <div key={row} className={meterGridClass}>
          <Skeleton shape="text" className="h-2.5 w-full" />
          <Skeleton className="h-1.5 w-full rounded-full" />
          <Skeleton shape="text" className="h-2.5 w-full" />
        </div>
      ))}
    </div>
  )
}

type BoundaryProps = { children: ReactNode; onRetry: () => void }
type BoundaryState = { hasError: boolean }

class UsageCardErrorBoundary extends Component<BoundaryProps, BoundaryState> {
  state: BoundaryState = { hasError: false }
  static getDerivedStateFromError(): BoundaryState {
    return { hasError: true }
  }
  componentDidCatch(error: Error) {
    console.warn('Usage card render failed', error)
  }
  render() {
    if (this.state.hasError) {
      // Retry delegates to the parent so it can bump the attempt
      // counter — that key change is what actually remounts the
      // boundary (clearing hasError) and the inner card (re-running
      // useQuery). Toggling local state here alone would clear the
      // fallback but leave the same broken inner element mounted.
      return (
        <section className={usagePanelClasses}>
          <p className="font-mono text-mono text-muted-strong">
            Couldn't display usage stats.{' '}
            <button type="button" className="underline" onClick={this.props.onRetry}>
              Retry
            </button>
          </p>
        </section>
      )
    }
    return this.props.children
  }
}
