import type { CSSProperties } from 'react'
import type { SidebarEntry } from '@/lib/types'
import type { NotchState } from '../api/notch-ipc'
import type { LimitMeter, ProfileMeters } from '../lib/profile-meters'

import { Suspense, useEffect, useRef, useState } from 'react'

import { useQueries, useQueryClient } from '@tanstack/react-query'

import { fetchUsableUsage, UsageUnavailableError } from '@/features/profiles/api/use-profile-usage'
import { appFromEntry, entryId, useSidebarEntries } from '@/features/profiles/api/use-sidebar-entries'
import { appSpecs } from '@/lib/app-registry'
import { useAppState } from '@/lib/app-state/use-app-state'
import { queryKeys } from '@/lib/query/keys'

import {
  getNotchState,
  notchHidden,
  onNotchState,
  openProfileFromNotch,
  setNotchContentHeight,
  setNotchWingWidth,
} from '../api/notch-ipc'
import { formatResetsIn, meterLevel, profileMeters } from '../lib/profile-meters'
import { LedBar } from './led-bar'

/**
 * How long a reading stays fresh. The panel fetches only while it is open —
 * someone is looking at it — so hovering the notch twice in a minute costs
 * nothing extra, and nothing polls while nobody is around.
 */
const usageStaleMs = 60_000

/**
 * The notch HUD: a black panel in the notch of the built-in display. Folded,
 * it shows the most-used weekly limit in its wing; open, one weekly meter per
 * profile with usage.
 */
export function NotchHud() {
  const queryClient = useQueryClient()
  const [state, setState] = useState<NotchState>(notchHidden)

  // The shell owns the geometry; the webview follows its events. The first
  // read covers a panel that opened before this listener existed.
  useEffect(() => {
    let current = true
    void getNotchState().then((initial) => {
      if (current) {
        setState((previous) => (previous.phase === 'hidden' ? initial : previous))
      }
    })
    const unlisten = onNotchState((next) => {
      if (next.phase === 'expanded') {
        // Profiles may have been added or renamed in the main window since
        // the panel last opened.
        void queryClient.invalidateQueries({ queryKey: queryKeys.profiles.all })
      }
      setState(next)
    })
    return () => {
      current = false
      void unlisten.then((stop) => stop())
    }
  }, [queryClient])

  if (state.phase === 'hidden') {
    return null
  }
  return (
    <Suspense fallback={<NotchFrame state={state} entries={[]} />}>
      <NotchPanel state={state} />
    </Suspense>
  )
}

type Row = {
  entry: SidebarEntry
  meters: ProfileMeters | null
  /** Why there is no meter, when the fetch failed and nothing is cached. */
  problem: string | null
}

function NotchPanel({ state }: { state: NotchState }) {
  const { state: appState } = useAppState()
  const entries = useSidebarEntries().filter((entry) => {
    if (appState.notchHiddenProfiles.includes(entryId(entry))) {
      return false
    }
    const surfaces = entry.kind === 'managed' ? entry.profile.surfaces : entry.entry.surfaces
    return surfaces.cli && appSpecs[appFromEntry(entry)].hasUsage
  })
  const results = useQueries({
    queries: entries.map((entry) => ({
      queryKey: queryKeys.profileUsage(entryId(entry)),
      queryFn: () => fetchUsableUsage(entryId(entry)),
      enabled: state.phase === 'expanded',
      staleTime: usageStaleMs,
      retry: false,
    })),
  })
  const now = new Date()
  const rows: Array<Row> = entries.map((entry, index) => {
    const result = results[index]
    const quota = result?.data?.quota
    return {
      entry,
      meters: quota ? profileMeters(appFromEntry(entry), quota, now) : null,
      problem: quota ? null : problemText(result?.error),
    }
  })
  return <NotchFrame state={state} entries={rows} />
}

function NotchFrame({ state, entries }: { state: NotchState; entries: Array<Row> }) {
  const rowsRef = useRef<HTMLDivElement>(null)
  const open = state.phase === 'expanded'

  // The rows render folded too, clipped by the window, so their height is
  // known before the panel opens and the open is a single resize. They sit
  // inside the scrolling body, which the folded window squeezes, so their
  // own height stays the natural one.
  useEffect(() => {
    const rows = rowsRef.current
    if (!rows) {
      return
    }
    const observer = new ResizeObserver(() => {
      void setNotchContentHeight(rows.offsetHeight)
    })
    observer.observe(rows)
    return () => observer.disconnect()
  }, [])

  // Measure again as the panel opens, so a reading the shell missed while
  // folded cannot leave the open panel clipped.
  useEffect(() => {
    const rows = rowsRef.current
    if (open && rows) {
      void setNotchContentHeight(rows.offsetHeight)
    }
  }, [open])

  const style = {
    '--notch-width': `${state.notch}px`,
    '--notch-height': `${state.height}px`,
    '--notch-wing': `${state.wing}px`,
    '--notch-fillet': `${state.fillet}px`,
    width: state.bodyWidth,
  } as CSSProperties

  return (
    <div className={open ? 'notch-panel notch-panel-open' : 'notch-panel'} style={style}>
      <ProfileSummary entries={entries} />
      <div className="notch-body" style={{ maxHeight: state.bodyMaxHeight }}>
        <div ref={rowsRef} className="notch-rows">
          {entries.length === 0 ? <p className="notch-empty">No profiles with usage</p> : null}
          {entries.map((row) => (
            <ProfileMeter key={entryId(row.entry)} row={row} />
          ))}
        </div>
      </div>
    </div>
  )
}

/** The space between a wing's chips and the notch, and the screen side. */
const wingPadding = 10

/**
 * The folded notch's summary: a chip per profile — its colour and weekly
 * percentage — split either side of the camera. The shell sizes the wings
 * to fit the wider side.
 */
function ProfileSummary({ entries }: { entries: Array<Row> }) {
  const leftRef = useRef<HTMLSpanElement>(null)
  const rightRef = useRef<HTMLSpanElement>(null)
  const half = Math.ceil(entries.length / 2)

  useEffect(() => {
    const left = leftRef.current
    const right = rightRef.current
    if (!(left && right)) {
      return
    }
    const observer = new ResizeObserver(() => {
      void setNotchWingWidth(Math.max(left.offsetWidth, right.offsetWidth) + wingPadding * 2)
    })
    observer.observe(left)
    observer.observe(right)
    return () => observer.disconnect()
  }, [])

  return (
    <header className="notch-header">
      <div className="notch-wing notch-wing-left">
        <span ref={leftRef} className="notch-chips">
          {entries.slice(0, half).map((row) => (
            <SummaryChip key={entryId(row.entry)} row={row} />
          ))}
        </span>
      </div>
      <div />
      <div className="notch-wing notch-wing-right">
        <span ref={rightRef} className="notch-chips">
          {entries.slice(half).map((row) => (
            <SummaryChip key={entryId(row.entry)} row={row} />
          ))}
        </span>
      </div>
    </header>
  )
}

/** One profile in the folded summary: its swatch and weekly percentage. */
function SummaryChip({ row }: { row: Row }) {
  const percent = row.meters?.weekly?.percent ?? null
  const name = row.entry.kind === 'managed' ? row.entry.profile.name : row.entry.entry.name
  return (
    <span className="notch-chip" title={name}>
      <Swatch entry={row.entry} />
      <span className="notch-chip-figure" data-level={meterLevel(percent)}>
        {formatPercent(percent)}
      </span>
    </span>
  )
}

function ProfileMeter({ row }: { row: Row }) {
  const { entry, meters, problem } = row
  return (
    <button
      type="button"
      className="notch-row"
      onClick={() => {
        void openProfileFromNotch(entryId(entry))
      }}
    >
      <span className="notch-row-head">
        <Swatch entry={entry} />
        <span className="notch-name">{entry.kind === 'managed' ? entry.profile.name : entry.entry.name}</span>
      </span>
      {meters?.session ? <LimitBar meter={meters.session} problem={null} /> : null}
      <LimitBar meter={meters?.weekly ?? null} problem={problem} />
    </button>
  )
}

/** One limit: its caption and reset time over its LED bar. */
function LimitBar({ meter, problem }: { meter: LimitMeter | null; problem: string | null }) {
  const percent = meter?.percent ?? null
  return (
    <span className="notch-limit">
      <span className="notch-limit-caption">
        <span>{captionText(meter, problem)}</span>
        <span className="notch-figure" data-level={meterLevel(percent)}>
          {formatPercent(percent)}
        </span>
      </span>
      <LedBar percent={percent} elapsed={meter?.elapsed ?? null} level={meterLevel(percent)} />
    </span>
  )
}

/** A managed profile's colour; a stock entry, which has none, as an outline. */
function Swatch({ entry }: { entry: SidebarEntry }) {
  if (entry.kind === 'default') {
    return <span className="notch-swatch notch-swatch-outline" />
  }
  return <span className="notch-swatch" style={{ backgroundColor: entry.profile.color }} />
}

function formatPercent(percent: number | null): string {
  return percent === null ? '--' : `${Math.round(percent)}%`
}

/** Pure: a bar's caption — its window and when it resets, or why it is empty. */
function captionText(meter: LimitMeter | null, problem: string | null): string {
  if (problem) {
    return problem
  }
  const resetsIn = meter?.resetsAt ? formatResetsIn(meter.resetsAt, new Date()) : null
  const label = meter?.label ?? 'Week'
  return resetsIn ? `${label} · resets in ${resetsIn}` : label
}

/** Pure: a short reason for a profile with no reading, or null while loading. */
function problemText(error: unknown): string | null {
  if (!(error instanceof UsageUnavailableError)) {
    return error ? 'No reading' : null
  }
  if (error.code === 'no_credentials' || error.code === 'needs_login' || error.code === 'unauthorized') {
    return 'Sign-in needed'
  }
  if (error.code === 'rate_limited') {
    return 'Rate limited'
  }
  if (error.code === 'network') {
    return 'Offline'
  }
  return 'No reading'
}
