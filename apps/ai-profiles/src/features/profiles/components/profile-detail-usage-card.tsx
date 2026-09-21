import type { ReactNode } from 'react'
import type { AppId } from '@/lib/app-registry'
import type { ProfileUsage, QuotaError, RateLimitResetCredits, UsageWindow } from '@/lib/types'

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

type UsageDisplay = 'used' | 'remaining'
const usageDisplayKey = 'ai-profiles-codex-usage-display'
const UsageDisplayContext = createContext<UsageDisplay>('used')

function readUsageDisplay(): UsageDisplay {
  try {
    return window.localStorage.getItem(usageDisplayKey) === 'remaining' ? 'remaining' : 'used'
  } catch {
    return 'used'
  }
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
                  aria-label={display === 'used' ? 'Show remaining quota' : 'Show used quota'}
                  onClick={() => changeDisplay(display === 'used' ? 'remaining' : 'used')}
                  className="group relative inline-flex min-h-[22px] cursor-pointer items-center gap-1 rounded px-1 text-mono transition-colors hover:bg-ink/[0.06] hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
                >
                  {display === 'used' ? 'Used' : 'Remaining'}
                  <ArrowLeftRight aria-hidden size={10} className="opacity-60" />
                  <TooltipBubble>
                    {display === 'used' ? 'Switch to remaining quota' : 'Switch to used quota'}
                  </TooltipBubble>
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
function quotaErrorMessage(app: AppId, quotaError: QuotaError, cliCommand: string): string {
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

function Meters({ app, quota }: { app: AppId; quota: ProfileUsage['quota'] }) {
  // Codex primary/secondary are positions, not fixed time periods. A weekly-only
  // plan can put its weekly quota in primary. Never invent an absent window.
  if (app === 'codex') {
    return (
      <div className="flex flex-col gap-2">
        {(['primary', 'secondary'] as const).map((slot) => {
          const window = quota?.[slot]
          if (!window) return null
          const minutes = window.windowDurationMins
          const label =
            minutes === 10080
              ? 'Weekly'
              : minutes == null
                ? 'Usage window'
                : minutes % 60 === 0
                  ? `${minutes / 60}-hour window`
                  : `${minutes}-minute window`
          const shortLabel =
            minutes === 10080
              ? 'W'
              : minutes == null
                ? 'Usage'
                : minutes % 60 === 0
                  ? `${minutes / 60}h`
                  : `${minutes}m`
          return (
            <Meter
              key={slot}
              label={label}
              shortLabel={shortLabel}
              meterWindow={window}
              showDailySegments={minutes === 10080}
              paceWindowMins={minutes}
            />
          )
        })}
        <AvailableResets resets={quota?.rateLimitResetCredits} />
      </div>
    )
  }
  const usageCopy = appSpecs[app].usage
  const secondaryExtra = quota?.secondaryExtra ?? null
  // The third "Sonnet-style" meter only exists for apps that define its
  // labels (Claude). Within that, skip the row when the user hasn't
  // touched it this window (utilization explicitly 0) so the card stays
  // focused. Unknown utilization (null) is kept visible — we'd rather
  // show a placeholder than silently drop a window we lack data for.
  const showExtra =
    usageCopy?.secondaryExtraLabel != null && secondaryExtra !== null && secondaryExtra.utilization !== 0
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
      {showExtra ? (
        <Meter
          showDailySegments
          paceWindowMins={10080}
          label={usageCopy?.secondaryExtraLabel ?? 'Weekly Sonnet'}
          shortLabel={usageCopy?.secondaryExtraShortLabel ?? 'WS'}
          meterWindow={secondaryExtra}
        />
      ) : null}
    </div>
  )
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

function Meter({
  label,
  shortLabel,
  meterWindow,
  showDailySegments = false,
  paceWindowMins,
}: {
  label: string
  shortLabel: string
  meterWindow: UsageWindow | null
  showDailySegments?: boolean
  paceWindowMins?: number | null
}) {
  // utilization comes from the API on a 0..=100 percentage scale and
  // may exceed 100 when the user is over-limit. We show the literal
  // value in the label but cap the visual bar fill at 100%.
  const utilization = meterWindow?.utilization ?? null
  const display = useContext(UsageDisplayContext)
  const usedPercent = utilization === null ? null : Math.round(utilization)
  const percent = usedPercent === null ? null : display === 'remaining' ? Math.max(0, 100 - usedPercent) : usedPercent
  const fillPercent = percent === null ? 0 : Math.min(100, Math.max(0, percent))
  const tone = usedPercent === null ? 'muted' : usedPercent < 50 ? 'ok' : usedPercent < 80 ? 'warn' : 'crit'
  const barClass =
    tone === 'ok' ? 'bg-green' : tone === 'warn' ? 'bg-amber' : tone === 'crit' ? 'bg-red' : 'bg-muted-strong'
  const resetLabel = formatReset(meterWindow?.resetsAt ?? null)
  const pacePercent = computePacePercent(meterWindow?.resetsAt ?? null, paceWindowMins)

  return (
    <div className={meterGridClass}>
      <span className="font-mono text-mono text-muted-strong">
        <span className="lg:hidden">{shortLabel}</span>
        <span className="hidden lg:inline">{label}</span>
      </span>
      <div className="relative">
        <div
          role="progressbar"
          aria-valuenow={percent ?? undefined}
          aria-valuetext={percent === null ? undefined : `${percent}% ${display}`}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-label={label}
          className="relative h-1.5 overflow-hidden rounded-full bg-cream-3"
        >
          <div className={`h-full rounded-full ${barClass}`} style={{ width: `${fillPercent}%` }} />
          {showDailySegments ? <DaySeparators /> : null}
        </div>
        {pacePercent === null ? null : (
          <PaceMarker
            percent={display === 'remaining' ? 100 - pacePercent : pacePercent}
            remaining={display === 'remaining'}
          />
        )}
      </div>
      <span className="text-right font-mono text-mono tabular-nums text-muted-strong">
        {percent === null ? '—' : `${percent}%`}
        {resetLabel ? (
          <span className="group relative inline-block">
            {` · ${resetLabel.relative}`}
            <TooltipBubble>{resetLabel.absolute}</TooltipBubble>
          </span>
        ) : null}
      </span>
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
