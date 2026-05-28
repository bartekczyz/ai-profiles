import type { ReactNode } from 'react'
import type { ProfileUsage, UsageWindow } from '@/lib/types'

import { Component, useEffect, useState } from 'react'

import { RefreshCw } from 'lucide-react'

import { refetchIntervalMs, useProfileUsage } from '../api/use-profile-usage'

type Props = {
  profileId: string
  cliEnabled: boolean
}

export function ProfileDetailUsageCard({ profileId, cliEnabled }: Props) {
  // Bumped by the in-boundary Retry button to force the inner query
  // to re-run after a render-time crash. We use it (alongside profileId)
  // as the key on the boundary itself, so switching profiles or hitting
  // Retry remounts the boundary — its hasError state resets along with
  // the inner useQuery's cache subscription.
  const [attempt, setAttempt] = useState(0)
  if (!cliEnabled) {
    return null
  }
  return (
    <UsageCardErrorBoundary key={`${profileId}:${attempt}`} onRetry={() => setAttempt((value) => value + 1)}>
      <UsageCardInner profileId={profileId} />
    </UsageCardErrorBoundary>
  )
}

function UsageCardInner({ profileId }: { profileId: string }) {
  const { data, isLoading, isFetching, isError, dataUpdatedAt, refetch } = useProfileUsage(profileId)

  return (
    <section className="mb-6 rounded-md border border-border p-4">
      <header className="mb-3 flex items-center justify-between">
        <div className="font-mono text-eyebrow font-medium uppercase tracking-[0.1em] text-muted-strong">Usage</div>
        <div className="flex items-center gap-2">
          <RefreshCountdown isFetching={isFetching} dataUpdatedAt={dataUpdatedAt} />
          <button
            type="button"
            aria-label="Refresh usage"
            disabled={isFetching}
            onClick={() => refetch()}
            className="cursor-pointer text-muted-strong hover:text-fg disabled:cursor-default disabled:opacity-50"
          >
            <RefreshCw size={14} className={isFetching ? 'animate-spin' : undefined} />
          </button>
        </div>
      </header>

      {isLoading ? <MetersSkeleton /> : <Body usage={data ?? null} isError={isError} />}
    </section>
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
  const remainingMs = dataUpdatedAt + refetchIntervalMs - Date.now()
  const label = formatRefreshIn(remainingMs)
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

function Body({ usage, isError }: { usage: ProfileUsage | null; isError: boolean }) {
  if (isError) {
    return <p className="font-mono text-mono text-muted-strong">Couldn't load usage stats.</p>
  }
  if (!usage) {
    return <MetersSkeleton />
  }
  const quotaMessage = quotaErrorMessage(usage.quotaError, usage.quota)
  if (quotaMessage) {
    return <p className="font-mono text-mono text-muted-strong">{quotaMessage}</p>
  }
  return <Meters quota={usage.quota} />
}

// Returns the message to show in place of the meters, or null if the
// meters should render. We treat "quota is null AND no recognised error"
// the same as `unknown` so we never silently render empty bars when the
// backend gave us nothing usable.
function quotaErrorMessage(quotaError: ProfileUsage['quotaError'], quota: ProfileUsage['quota']): string | null {
  if (quotaError === 'no_credentials') {
    return 'Sign in to Claude Code once with this profile to see usage.'
  }
  if (quotaError === 'unauthorized') {
    // Not a real "session expired" — Claude Code's short-lived access
    // token rolls over every ~hour and is silently refreshed the next
    // time you invoke `claude`. Same step also handles the rarer case
    // where the token was actually revoked (claude will prompt re-login).
    return 'Token refresh needed — run `claude` in a terminal once, then retry.'
  }
  if (quotaError === 'rate_limited') {
    return 'Rate limited by Anthropic. Try again in a few minutes.'
  }
  if (quotaError === 'network') {
    return "Couldn't reach Anthropic — check your connection and retry."
  }
  if (quotaError === 'unknown') {
    return "Couldn't load usage stats. Try again."
  }
  if (!quota) {
    return "Couldn't load usage stats. Try again."
  }
  return null
}

function Meters({ quota }: { quota: ProfileUsage['quota'] }) {
  return (
    <div className="flex flex-col gap-2">
      <Meter label="5-hour window" shortLabel="5h" meterWindow={quota?.fiveHour ?? null} />
      <Meter showDailySegments label="Weekly" shortLabel="W" meterWindow={quota?.sevenDay ?? null} />
      <Meter showDailySegments label="Weekly Sonnet" shortLabel="WS" meterWindow={quota?.sevenDaySonnet ?? null} />
    </div>
  )
}

// Layout: [label] [bar (1fr)] [trailing text fixed width]. The fixed
// trailing column keeps every bar exactly the same width across rows
// and reserves space for the longest "100% · resets in 23h 59m"
// string (~22 mono chars ≈ 180px). The label column shrinks at narrow
// viewports so the bar still has room to breathe.
const meterGridClass = 'grid grid-cols-[32px_1fr_180px] items-center gap-2 lg:grid-cols-[140px_1fr_180px] lg:gap-3'

function Meter({
  label,
  shortLabel,
  meterWindow,
  showDailySegments = false,
}: {
  label: string
  shortLabel: string
  meterWindow: UsageWindow | null
  showDailySegments?: boolean
}) {
  // utilization comes from the API on a 0..=100 percentage scale and
  // may exceed 100 when the user is over-limit. We show the literal
  // value in the label but cap the visual bar fill at 100%.
  const utilization = meterWindow?.utilization ?? null
  const percent = utilization === null ? null : Math.round(utilization)
  const fillPercent = percent === null ? 0 : Math.min(100, Math.max(0, percent))
  const tone = percent === null ? 'muted' : percent < 50 ? 'ok' : percent < 80 ? 'warn' : 'crit'
  const barClass =
    tone === 'ok' ? 'bg-green' : tone === 'warn' ? 'bg-amber' : tone === 'crit' ? 'bg-red' : 'bg-muted-strong'
  const resetsLabel = formatReset(meterWindow?.resetsAt ?? null)
  const pacePercent = showDailySegments ? computeWeeklyPacePercent(meterWindow?.resetsAt ?? null) : null

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
          aria-valuemin={0}
          aria-valuemax={100}
          aria-label={label}
          className="relative h-2 overflow-hidden rounded bg-cream-2"
        >
          <div className={`h-full ${barClass}`} style={{ width: `${fillPercent}%` }} />
          {showDailySegments ? <DaySeparators /> : null}
        </div>
        {pacePercent === null ? null : <PaceMarker percent={pacePercent} />}
      </div>
      <span className="text-right font-mono text-mono tabular-nums text-muted-strong">
        {percent === null ? '—' : `${percent}%`}
        {resetsLabel ? ` · ${resetsLabel}` : ''}
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

function PaceMarker({ percent }: { percent: number }) {
  const clamped = Math.min(100, Math.max(0, percent))
  return (
    <div
      className="group absolute -top-0.5 flex h-3 w-3 items-center justify-center"
      style={{ left: `calc(${clamped}% - 6px)` }}
    >
      <div aria-hidden className="pointer-events-none h-full w-0.5 rounded-sm bg-ink" />
      <div
        role="tooltip"
        className="pointer-events-none absolute bottom-full left-1/2 z-10 mb-1.5 -translate-x-1/2 whitespace-nowrap rounded bg-ink px-2 py-1 font-mono text-mono text-cream opacity-0 shadow-md transition-opacity group-hover:opacity-100"
      >
        Even daily pace · {Math.round(clamped)}%
      </div>
    </div>
  )
}

// Weekly limits reset at a fixed time on a 7-day cycle, so the "expected"
// burn position is just how far we've travelled from the previous reset
// (resetsAt - 7d) toward the next one. Returns null when the input is
// missing or out of range.
function computeWeeklyPacePercent(resetsAt: string | null): number | null {
  if (!resetsAt) {
    return null
  }
  const resetTime = new Date(resetsAt).getTime()
  if (Number.isNaN(resetTime)) {
    return null
  }
  const windowMs = 7 * 24 * 60 * 60 * 1000
  const timeRemaining = resetTime - Date.now()
  if (timeRemaining <= 0) {
    return 100
  }
  if (timeRemaining >= windowMs) {
    return 0
  }
  return ((windowMs - timeRemaining) / windowMs) * 100
}

function formatReset(resetsAt: string | null): string | null {
  if (!resetsAt) {
    return null
  }
  const date = new Date(resetsAt)
  if (Number.isNaN(date.getTime())) {
    return null
  }
  const deltaMs = date.getTime() - Date.now()
  if (deltaMs <= 0) {
    return 'resets soon'
  }
  const hours = Math.floor(deltaMs / (60 * 60 * 1000))
  const minutes = Math.floor((deltaMs % (60 * 60 * 1000)) / (60 * 1000))
  if (hours >= 24) {
    const days = Math.floor(hours / 24)
    return `resets in ${days}d`
  }
  if (hours >= 1) {
    return `resets in ${hours}h ${minutes}m`
  }
  return `resets in ${minutes}m`
}

function MetersSkeleton() {
  return (
    <div className="flex flex-col gap-2">
      {[0, 1, 2].map((row) => (
        <div key={row} className={meterGridClass}>
          <span className="h-3 w-full rounded bg-cream-2" />
          <span className="h-2 w-full rounded bg-cream-2" />
          <span className="h-3 w-full rounded bg-cream-2" />
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
        <section className="mb-6 rounded-md border border-border p-4">
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
