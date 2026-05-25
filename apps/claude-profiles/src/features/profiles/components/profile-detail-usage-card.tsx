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
  if (!cliEnabled) {
    return null
  }
  return (
    <UsageCardErrorBoundary>
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
  if (usage.quotaError === 'no_credentials') {
    return (
      <p className="font-mono text-mono text-muted-strong">
        Sign in to Claude Code once with this profile to see usage.
      </p>
    )
  }
  if (usage.quotaError === 'unauthorized') {
    return (
      <p className="font-mono text-mono text-muted-strong">
        Session expired — sign in again with this profile to refresh usage.
      </p>
    )
  }
  return <Meters quota={usage.quota} />
}

function Meters({ quota }: { quota: ProfileUsage['quota'] }) {
  return (
    <div className="flex flex-col gap-2">
      <Meter label="5-hour window" shortLabel="5h" meterWindow={quota?.fiveHour ?? null} />
      <Meter label="Weekly" shortLabel="W" meterWindow={quota?.sevenDay ?? null} />
      <Meter label="Weekly Sonnet" shortLabel="WS" meterWindow={quota?.sevenDaySonnet ?? null} />
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
}: {
  label: string
  shortLabel: string
  meterWindow: UsageWindow | null
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

  return (
    <div className={meterGridClass}>
      <span className="font-mono text-mono text-muted-strong">
        <span className="lg:hidden">{shortLabel}</span>
        <span className="hidden lg:inline">{label}</span>
      </span>
      <div
        role="progressbar"
        aria-valuenow={percent ?? undefined}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={label}
        className="h-2 overflow-hidden rounded bg-cream-2"
      >
        <div className={`h-full ${barClass}`} style={{ width: `${fillPercent}%` }} />
      </div>
      <span className="text-right font-mono text-mono tabular-nums text-muted-strong">
        {percent === null ? '—' : `${percent}%`}
        {resetsLabel ? ` · ${resetsLabel}` : ''}
      </span>
    </div>
  )
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

type BoundaryState = { hasError: boolean }

class UsageCardErrorBoundary extends Component<{ children: ReactNode }, BoundaryState> {
  state: BoundaryState = { hasError: false }
  static getDerivedStateFromError(): BoundaryState {
    return { hasError: true }
  }
  componentDidCatch(error: Error) {
    console.warn('Usage card render failed', error)
  }
  render() {
    if (this.state.hasError) {
      return (
        <section className="mb-6 rounded-md border border-border p-4">
          <p className="font-mono text-mono text-muted-strong">
            Couldn't display usage stats.{' '}
            <button type="button" className="underline" onClick={() => this.setState({ hasError: false })}>
              Retry
            </button>
          </p>
        </section>
      )
    }
    return this.props.children
  }
}
