import type { ReactNode } from 'react'
import type { ProfileUsage } from '@/lib/types'

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { appSpecs } from '@/lib/app-registry'
import { openCliLogin } from '@/lib/commands'

import { UsageUnavailableError, useProfileUsage } from '../api/use-profile-usage'
import {
  codexDisplayToggleCopy,
  codexMeterRows,
  codexWindowLabel,
  displayPacePercent,
  displayPercent,
  Meters,
  meterFillPercent,
  meterTone,
  ProfileDetailUsageCard,
  quotaErrorMessage,
  toggleUsageDisplay,
  usedPercentFromUtilization,
  visibleScopedWeekly,
} from './profile-detail-usage-card'

// Partial mock: keep the real `refetchIntervalMs` and `UsageUnavailableError`
// (the card does `error instanceof UsageUnavailableError`), stub only the hook.
vi.mock('../api/use-profile-usage', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/use-profile-usage')>()
  return { ...actual, useProfileUsage: vi.fn() }
})

vi.mock('@/lib/commands', () => ({ openCliLogin: vi.fn() }))

function makeUsage(overrides: Partial<ProfileUsage> = {}): ProfileUsage {
  return {
    quota: {
      // Utilization is a 0..=100 percentage — matches Anthropic's response.
      primary: { utilization: 63, resetsAt: null },
      secondary: { utilization: 21, resetsAt: null },
      scopedWeekly: [{ utilization: 8, resetsAt: null }],
    },
    quotaError: null,
    fetchedAt: '2099-01-01T00:00:00Z',
    ...overrides,
  }
}

/**
 * Stubs the usage query with a settled snapshot carrying `quota`.
 */
function mockUsage(quota: NonNullable<ProfileUsage['quota']>) {
  vi.mocked(useProfileUsage).mockReturnValue({
    data: makeUsage({ quota }),
    isLoading: false,
    isFetching: false,
    refetch: vi.fn(),
  } as unknown as ReturnType<typeof useProfileUsage>)
}

function renderWithQuery(children: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(<QueryClientProvider client={client}>{children}</QueryClientProvider>)
}

describe('ProfileDetailUsageCard', () => {
  beforeEach(() => window.localStorage.removeItem('ai-profiles-codex-usage-display'))

  it('switches to remaining and restores the latest choice after remount', () => {
    vi.mocked(useProfileUsage).mockReturnValue({
      data: makeUsage({
        quota: {
          primary: { utilization: 85, resetsAt: null, windowDurationMins: 10080 },
          secondary: null,
          scopedWeekly: [],
        },
      }),
      isFetching: false,
      refetch: vi.fn(),
    } as unknown as ReturnType<typeof useProfileUsage>)
    const first = renderWithQuery(<ProfileDetailUsageCard app="codex" profileId="p1" cliEnabled />)
    fireEvent.click(screen.getByRole('button', { name: 'Show remaining quota' }))
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '15')
    expect(screen.getByRole('progressbar').firstElementChild).toHaveClass('bg-red')
    expect(screen.getByRole('progressbar').firstElementChild).toHaveStyle({ width: '15%' })
    first.unmount()
    const second = renderWithQuery(<ProfileDetailUsageCard app="codex" profileId="p1" cliEnabled />)
    expect(screen.getByRole('button', { name: 'Show used quota' })).toHaveTextContent('Remaining')
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '15')
    fireEvent.click(screen.getByRole('button', { name: 'Show used quota' }))
    second.unmount()
    renderWithQuery(<ProfileDetailUsageCard app="codex" profileId="p1" cliEnabled />)
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '85')
  })

  it.each([
    [
      2,
      [{ title: 'Full reset', status: 'available', expiresAt: 1893456000 }],
      'Expiry details unavailable for 1 reset.',
    ],
    [2, null, 'Expiry details unavailable for 2 resets.'],
    [0, [], 'No resets available'],
  ])('shows the authoritative Codex reset count %s', (availableCount, credits, detail) => {
    const usage = makeUsage()
    vi.mocked(useProfileUsage).mockReturnValue({
      data: { ...usage, quota: { ...usage.quota, rateLimitResetCredits: { availableCount, credits } } },
      isFetching: false,
      refetch: vi.fn(),
    } as unknown as ReturnType<typeof useProfileUsage>)
    renderWithQuery(<ProfileDetailUsageCard app="codex" profileId="p1" cliEnabled />)
    if (availableCount === 0) {
      expect(screen.queryByText(detail)).not.toBeInTheDocument()
    } else {
      expect(screen.getByText(detail)).toBeInTheDocument()
    }
    if (availableCount) expect(screen.getByText('2 resets available')).toBeInTheDocument()
    if (credits?.length) {
      expect(screen.getByText(/^expires in \d+d$/)).toBeInTheDocument()
      expect(screen.getByRole('tooltip', { name: /2030/ })).toBeInTheDocument()
    }
  })

  it.each([
    [10080, 'Weekly'],
    [300, '5-hour window'],
    [60, '1-hour window'],
    [null, 'Usage window'],
  ])('labels a single Codex window by duration %s and hides absent windows', (duration, label) => {
    vi.mocked(useProfileUsage).mockReturnValue({
      data: makeUsage({
        quota: {
          primary: { utilization: 14, resetsAt: null, windowDurationMins: duration },
          secondary: null,
          scopedWeekly: [],
        },
      }),
      isFetching: false,
      refetch: vi.fn(),
    } as unknown as ReturnType<typeof useProfileUsage>)
    renderWithQuery(<ProfileDetailUsageCard app="codex" profileId="p1" cliEnabled />)
    expect(screen.getAllByRole('progressbar')).toHaveLength(1)
    expect(screen.getByRole('progressbar', { name: label })).toHaveAttribute('aria-valuenow', '14')
  })

  it('renders three progressbars with the right aria-valuenow', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage(),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    const bars = screen.getAllByRole('progressbar')
    expect(bars).toHaveLength(3)
    expect(bars[0]).toHaveAttribute('aria-valuenow', '63')
    expect(bars[1]).toHaveAttribute('aria-valuenow', '21')
    expect(bars[2]).toHaveAttribute('aria-valuenow', '8')
  })

  it('omits aria-valuenow when utilization is null', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage({
        quota: {
          primary: { utilization: null, resetsAt: null },
          secondary: { utilization: 21, resetsAt: null },
          scopedWeekly: [{ utilization: 8, resetsAt: null }],
        },
      }),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    const bars = screen.getAllByRole('progressbar')
    expect(bars[0]).not.toHaveAttribute('aria-valuenow')
  })

  it('hides the meters when the query errors with no_credentials', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: undefined,
      error: new UsageUnavailableError('no_credentials'),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    expect(screen.queryByRole('progressbar')).toBeNull()
  })

  it.each([
    ['unauthorized', /token refresh needed/i],
    ['needs_login', /session expired/i],
    ['rate_limited', /rate limited/i],
    ['network', /couldn't reach anthropic/i],
    ['unknown', /couldn't load usage stats/i],
  ] as const)('shows an explicit message and no meters when the query errors with %s', (code, expected) => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: undefined,
      error: new UsageUnavailableError(code),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    expect(screen.queryByRole('progressbar')).toBeNull()
    expect(screen.getByText(expected)).toBeInTheDocument()
  })

  it('shows the unknown-error message when the query errors with unknown', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: undefined,
      error: new UsageUnavailableError('unknown'),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    expect(screen.queryByRole('progressbar')).toBeNull()
    expect(screen.getByText(/couldn't load usage stats/i)).toBeInTheDocument()
  })

  it('calls refetch when the refresh button is clicked', () => {
    const refetch = vi.fn()
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage(),
      isLoading: false,
      isFetching: false,
      refetch,
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    fireEvent.click(screen.getByRole('button', { name: /refresh/i }))
    expect(refetch).toHaveBeenCalledTimes(1)
  })

  it('returns null when CLI is not enabled for the profile', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage(),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    const { container } = renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={false} />)
    expect(container.firstChild).toBeNull()
  })

  it('shows "refresh in Xm" countdown derived from dataUpdatedAt', () => {
    // Fetched just now; with a 5-minute refetch interval the countdown
    // should land at "4m" (5 minutes minus the few ms between Date.now()
    // calls). Loose regex tolerates the off-by-one between 4m and 5m.
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage(),
      isLoading: false,
      isFetching: false,
      dataUpdatedAt: Date.now(),
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    expect(screen.getByText(/refresh in [45]m/)).toBeInTheDocument()
  })

  it('shows "refreshing…" while a fetch is in flight', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage(),
      isLoading: false,
      isFetching: true,
      dataUpdatedAt: Date.now(),
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    expect(screen.getByText(/refreshing/)).toBeInTheDocument()
  })

  it('keeps showing the meters with a "couldn\'t refresh" note when a refresh errors but data is cached', () => {
    // Stale-while-revalidate: a rate-limited refresh must not hide the
    // last-known meters; it adds a quiet note instead of the full message.
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage(),
      error: new UsageUnavailableError('rate_limited'),
      isLoading: false,
      isFetching: false,
      dataUpdatedAt: Date.now(),
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    expect(screen.getAllByRole('progressbar')).toHaveLength(3)
    expect(screen.getByText(/couldn't refresh/i)).toBeInTheDocument()
  })

  it('shows "updated Xh ago" for data older than the refresh interval', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage(),
      isLoading: false,
      isFetching: false,
      dataUpdatedAt: Date.now() - 2 * 60 * 60 * 1000,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    expect(screen.getByText(/updated 2h ago/i)).toBeInTheDocument()
  })

  it('clears the error fallback when profileId changes', () => {
    const usageMock = useProfileUsage as ReturnType<typeof vi.fn>
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => undefined)

    // While profile p1 is mounted, throw to drive the boundary into
    // its hasError state. Switching to p2 changes the boundary's `key`
    // so it remounts with a fresh state.
    usageMock.mockImplementation((profileId: string) => {
      if (profileId === 'p1') {
        throw new Error('boom')
      }
      return {
        data: makeUsage(),
        isLoading: false,
        isFetching: false,
        refetch: vi.fn(),
      }
    })

    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const { rerender } = render(
      <QueryClientProvider client={client}>
        <ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />
      </QueryClientProvider>,
    )
    expect(screen.getByText(/couldn't display usage stats/i)).toBeInTheDocument()

    rerender(
      <QueryClientProvider client={client}>
        <ProfileDetailUsageCard app="claude" profileId="p2" cliEnabled={true} />
      </QueryClientProvider>,
    )
    expect(screen.queryByText(/couldn't display usage stats/i)).toBeNull()
    expect(screen.getAllByRole('progressbar')).toHaveLength(3)

    consoleError.mockRestore()
    consoleWarn.mockRestore()
  })

  it('Retry recovers the card by remounting the inner query', () => {
    const usageMock = useProfileUsage as ReturnType<typeof vi.fn>
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => undefined)

    // Throw until the Retry click flips the flag — Retry bumps the
    // parent's `attempt` counter, which changes the boundary's key,
    // which remounts the boundary AND the inner query.
    let throwing = true
    usageMock.mockImplementation(() => {
      if (throwing) {
        throw new Error('boom')
      }
      return {
        data: makeUsage(),
        isLoading: false,
        isFetching: false,
        refetch: vi.fn(),
      }
    })

    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    expect(screen.getByText(/couldn't display usage stats/i)).toBeInTheDocument()

    throwing = false
    fireEvent.click(screen.getByRole('button', { name: /retry/i }))
    expect(screen.queryByText(/couldn't display usage stats/i)).toBeNull()
    expect(screen.getAllByRole('progressbar')).toHaveLength(3)

    consoleError.mockRestore()
    consoleWarn.mockRestore()
  })

  it('renders a hover tooltip with the absolute reset time for each window that has resetsAt', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage({
        quota: {
          primary: { utilization: 40, resetsAt: '2099-06-15T14:30:00Z' },
          secondary: { utilization: 10, resetsAt: '2099-06-22T09:00:00Z' },
          scopedWeekly: [{ utilization: 5, resetsAt: null }],
        },
      }),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    // Each window with resetsAt contributes a tooltip alongside its
    // "resets in …" label. Tooltip content is the absolute datetime in
    // the "EEE d MMM, HH:mm" pattern (locale-stable across CI hosts;
    // hour digits and short day/month names are timezone-independent in
    // shape even if absolute values shift).
    const datetimePattern = /^[A-Z][a-z]{2} \d{1,2} [A-Z][a-z]{2}, \d{2}:\d{2}$/
    const tooltips = screen
      .getAllByRole('tooltip')
      .map((node) => node.textContent?.trim() ?? '')
      .filter((text) => datetimePattern.test(text))
    // 5h and weekly each emit one datetime tooltip; weekly Sonnet has
    // no resetsAt so contributes nothing. The PaceMarker on the weekly
    // window also emits a tooltip, but its content is "Even daily pace
    // · N%" which the pattern filter excludes.
    expect(tooltips).toHaveLength(2)
  })

  it('hides the Weekly Sonnet row when its utilization is zero', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage({
        quota: {
          primary: { utilization: 40, resetsAt: null },
          secondary: { utilization: 10, resetsAt: null },
          scopedWeekly: [{ utilization: 0, resetsAt: null }],
        },
      }),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    // Only 5-hour and weekly bars render; Weekly Sonnet is suppressed.
    expect(screen.getAllByRole('progressbar')).toHaveLength(2)
  })

  it('keeps the Weekly Sonnet row visible when its utilization is unknown (null)', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage({
        quota: {
          primary: { utilization: 40, resetsAt: null },
          secondary: { utilization: 10, resetsAt: null },
          scopedWeekly: [{ utilization: null, resetsAt: null }],
        },
      }),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    // Unknown utilization is not the same as zero — we still surface
    // the row (with a "—" placeholder) so the user can tell data is
    // missing rather than confused with "no usage this week".
    expect(screen.getAllByRole('progressbar')).toHaveLength(3)
  })

  it('omits the reset tooltip when resetsAt is null', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage({
        quota: {
          primary: { utilization: 40, resetsAt: null },
          secondary: { utilization: 10, resetsAt: null },
          scopedWeekly: [{ utilization: 5, resetsAt: null }],
        },
      }),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    const datetimePattern = /^[A-Z][a-z]{2} \d{1,2} [A-Z][a-z]{2}, \d{2}:\d{2}$/
    const tooltips = screen.queryAllByRole('tooltip')
    const datetimeTooltips = tooltips.filter((node) => datetimePattern.test((node.textContent ?? '').trim()))
    expect(datetimeTooltips).toHaveLength(0)
  })

  it('renders only two meters for a Codex profile even when a scoped weekly is present', () => {
    // Codex reports no per-model weekly sub-quota — its spec sets
    // hasScopedWeekly false, so the card must never render a third meter
    // regardless of the payload.
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage({
        quota: {
          primary: { utilization: 1, resetsAt: null },
          secondary: { utilization: 10, resetsAt: null },
          scopedWeekly: [{ utilization: 8, resetsAt: null }],
        },
      }),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="codex" profileId="p1" cliEnabled={true} />)
    expect(screen.getAllByRole('progressbar')).toHaveLength(2)
  })

  it('renders three meters for a Claude profile with a scoped weekly window', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage({
        quota: {
          primary: { utilization: 63, resetsAt: null },
          secondary: { utilization: 21, resetsAt: null },
          scopedWeekly: [{ utilization: 8, resetsAt: null }],
        },
      }),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    expect(screen.getAllByRole('progressbar')).toHaveLength(3)
  })

  it('names a scoped weekly meter from the server-supplied model', () => {
    // The model changes over time (Sonnet → Opus → Fable), so the row must
    // read its name off the payload rather than any hardcoded copy.
    mockUsage({
      primary: { utilization: 4, resetsAt: null },
      secondary: { utilization: 24, resetsAt: null },
      scopedWeekly: [{ utilization: 19, resetsAt: null, label: 'Fable' }],
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled />)
    expect(screen.getByRole('progressbar', { name: 'Weekly · Fable' })).toHaveAttribute('aria-valuenow', '19')
  })

  it('gives every scoped weekly its own meter', () => {
    mockUsage({
      primary: null,
      secondary: null,
      scopedWeekly: [
        { utilization: 19, resetsAt: null, label: 'Fable' },
        { utilization: 3, resetsAt: null, label: 'Opus' },
      ],
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled />)
    expect(screen.getByRole('progressbar', { name: 'Weekly · Fable' })).toHaveAttribute('aria-valuenow', '19')
    expect(screen.getByRole('progressbar', { name: 'Weekly · Opus' })).toHaveAttribute('aria-valuenow', '3')
  })

  it.each([
    ['no_credentials', /sign in to claude code once/i],
    ['needs_login', /session expired — run `claude`/i],
    ['unauthorized', /token refresh needed — run `claude`/i],
    ['forbidden', /blocked upstream/i],
    ['rate_limited', /rate limited/i],
    ['network', /couldn't reach anthropic/i],
    ['unknown', /couldn't load usage stats/i],
  ] as const)('quotaErrorMessage renders %s copy for claude', (code, expected) => {
    expect(quotaErrorMessage('claude', code, 'claude')).toMatch(expected)
  })

  it('quotaErrorMessage names the given CLI command for needs_login and unauthorized', () => {
    expect(quotaErrorMessage('claude', 'needs_login', 'claude-work')).toContain('claude-work')
    expect(quotaErrorMessage('claude', 'unauthorized', 'claude-work')).toContain('claude-work')
  })

  it('falls back to a neutral label when the scoped weekly names no model', () => {
    mockUsage({
      primary: null,
      secondary: null,
      scopedWeekly: [{ utilization: 19, resetsAt: null }],
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled />)
    expect(screen.getByRole('progressbar', { name: 'Weekly (scoped)' })).toHaveAttribute('aria-valuenow', '19')
  })

  it('drops a scoped weekly the user has not touched this window', () => {
    mockUsage({
      primary: { utilization: 4, resetsAt: null },
      secondary: { utilization: 24, resetsAt: null },
      scopedWeekly: [{ utilization: 0, resetsAt: null, label: 'Fable' }],
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled />)
    expect(screen.queryByRole('progressbar', { name: 'Weekly · Fable' })).not.toBeInTheDocument()
  })

  it('keeps a scoped weekly whose utilization is unknown', () => {
    // Null is missing data, not zero use — dropping it would hide a window
    // silently.
    mockUsage({
      primary: null,
      secondary: null,
      scopedWeekly: [{ utilization: null, resetsAt: null, label: 'Fable' }],
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled />)
    expect(screen.getByRole('progressbar', { name: 'Weekly · Fable' })).not.toHaveAttribute('aria-valuenow')
  })

  it('renders usage credits as money converted out of minor units', () => {
    mockUsage({
      primary: null,
      secondary: null,
      scopedWeekly: [],
      spend: { usedMinor: 7788, limitMinor: 30000, currency: 'GBP', exponent: 2, percent: 26 },
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled />)
    expect(screen.getByRole('progressbar', { name: 'Usage credits' })).toHaveAttribute('aria-valuenow', '26')
    expect(screen.getByText(/77\.88 of .*300\.00/)).toBeInTheDocument()
  })

  it('derives the credits percentage when the server omits it', () => {
    mockUsage({
      primary: null,
      secondary: null,
      scopedWeekly: [],
      spend: { usedMinor: 2500, limitMinor: 10000, currency: 'USD', exponent: 2, percent: null },
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled />)
    expect(screen.getByRole('progressbar', { name: 'Usage credits' })).toHaveAttribute('aria-valuenow', '25')
  })

  it('honours a zero-decimal currency', () => {
    mockUsage({
      primary: null,
      secondary: null,
      scopedWeekly: [],
      spend: { usedMinor: 900, limitMinor: 5000, currency: 'JPY', exponent: 0, percent: 18 },
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled />)
    expect(screen.getByText(/900 of .*5,000/)).toBeInTheDocument()
  })

  it('omits the credits row for an uncapped account', () => {
    // Without a cap there is nothing for the bar to fill against.
    mockUsage({
      primary: { utilization: 4, resetsAt: null },
      secondary: null,
      scopedWeekly: [],
      spend: { usedMinor: 500, limitMinor: null, currency: 'USD', exponent: 2, percent: null },
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled />)
    expect(screen.queryByRole('progressbar', { name: 'Usage credits' })).not.toBeInTheDocument()
  })

  it('omits the credits row when the profile reports no spend', () => {
    mockUsage({ primary: { utilization: 4, resetsAt: null }, secondary: null, scopedWeekly: [] })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled />)
    expect(screen.queryByRole('progressbar', { name: 'Usage credits' })).not.toBeInTheDocument()
  })

  it('names the profile CLI command when sign-in is needed', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: undefined,
      error: new UsageUnavailableError('needs_login'),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(
      <ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} cliCommand="claude-personal" />,
    )
    expect(screen.getByText(/claude-personal/)).toBeInTheDocument()
  })

  it('opens the profile CLI in Terminal when "Refresh sign-in" is clicked', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: undefined,
      error: new UsageUnavailableError('needs_login'),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(
      <ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} cliCommand="claude-personal" />,
    )
    fireEvent.click(screen.getByRole('button', { name: /refresh sign-in/i }))
    expect(openCliLogin).toHaveBeenCalledWith('p1')
  })

  it('falls back to the stock CLI binary name when no cliCommand is given', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: undefined,
      error: new UsageUnavailableError('unauthorized'),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="p1" cliEnabled={true} />)
    expect(screen.getByText(/run `claude` once/i)).toBeInTheDocument()
  })

  it('shows the Codex no-credentials copy from the app spec', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: undefined,
      error: new UsageUnavailableError('no_credentials'),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard app="codex" profileId="p1" cliEnabled={true} />)
    const expected = appSpecs.codex.usage?.noCredentials ?? ''
    expect(screen.getByText(expected)).toBeInTheDocument()
  })
})

describe('quota pace windows', () => {
  const now = Date.parse('2026-09-01T00:00:00Z')
  beforeEach(() => {
    vi.spyOn(Date, 'now').mockReturnValue(now)
    window.localStorage.removeItem('ai-profiles-codex-usage-display')
  })
  afterEach(() => vi.restoreAllMocks())

  function renderCodex(duration: number | null | undefined, reset: string | null) {
    vi.mocked(useProfileUsage).mockReturnValue({
      data: makeUsage({
        quota: {
          primary: { utilization: 15, resetsAt: reset, windowDurationMins: duration },
          secondary: null,
          scopedWeekly: [],
        },
      }),
      isFetching: false,
      refetch: vi.fn(),
    } as unknown as ReturnType<typeof useProfileUsage>)
    renderWithQuery(<ProfileDetailUsageCard app="codex" profileId="pace" cliEnabled />)
  }

  it.each([
    [43200, '2026-09-27T00:00:00Z', 13.333333, 1],
    [10080, '2026-09-04T12:00:00Z', 50, 7],
    [300, '2026-09-01T02:30:00Z', 50, 1],
    [43200, '2026-08-31T00:00:00Z', 100, 1],
    [43200, '2026-10-02T00:00:00Z', 0, 1],
  ])('uses duration %s for pace, independently of weekly separators', (duration, reset, expected, barChildren) => {
    renderCodex(duration, reset)
    const tooltip = screen.getByRole('tooltip', { name: `Even daily pace · ${Math.round(expected)}%` })
    const left = tooltip.parentElement?.style.left ?? ''
    expect(Number(left.match(/calc\(([-\d.]+)%/)?.[1])).toBeCloseTo(expected, 4)
    expect(screen.getByRole('progressbar').children).toHaveLength(barChildren)
  })

  it('inverts a 30-day marker in remaining mode', () => {
    renderCodex(43200, '2026-09-27T00:00:00Z')
    fireEvent.click(screen.getByRole('button', { name: 'Show remaining quota' }))
    const tooltip = screen.getByRole('tooltip', { name: 'Even daily pace · 87% remaining' })
    expect(Number(tooltip.parentElement?.style.left.match(/calc\(([-\d.]+)%/)?.[1])).toBeCloseTo(86.666667, 4)
  })

  it.each([
    [undefined, '2026-09-27T00:00:00Z'],
    [null, '2026-09-27T00:00:00Z'],
    [0, '2026-09-27T00:00:00Z'],
    [-1, '2026-09-27T00:00:00Z'],
    [Number.NaN, '2026-09-27T00:00:00Z'],
    [Number.POSITIVE_INFINITY, '2026-09-27T00:00:00Z'],
    [43200, null],
    [43200, 'invalid'],
  ])('hides pace for invalid duration/reset %s %s', (duration, reset) => {
    renderCodex(duration, reset)
    expect(screen.queryByRole('tooltip', { name: /Even daily pace/ })).not.toBeInTheDocument()
  })

  it('preserves both Claude weekly markers and their separators', () => {
    const weekly = { utilization: 15, resetsAt: '2026-09-04T12:00:00Z' }
    vi.mocked(useProfileUsage).mockReturnValue({
      data: makeUsage({ quota: { primary: null, secondary: weekly, scopedWeekly: [weekly] } }),
      isFetching: false,
      refetch: vi.fn(),
    } as unknown as ReturnType<typeof useProfileUsage>)
    renderWithQuery(<ProfileDetailUsageCard app="claude" profileId="pace" cliEnabled />)
    expect(screen.getAllByRole('tooltip', { name: 'Even daily pace · 50%' })).toHaveLength(2)
    const bars = screen.getAllByRole('progressbar')
    expect(bars[1].children).toHaveLength(7)
    expect(bars[2].children).toHaveLength(7)
  })
})

describe('Meters', () => {
  it('renders one meter per present Codex slot plus resets', () => {
    render(
      <Meters
        app="codex"
        quota={{
          primary: { utilization: 10, resetsAt: null, windowDurationMins: 10080 },
          secondary: null,
          scopedWeekly: [],
          rateLimitResetCredits: { availableCount: 1, credits: null },
        }}
      />,
    )
    expect(screen.getAllByRole('progressbar')).toHaveLength(1)
    expect(screen.getByRole('progressbar', { name: 'Weekly' })).toHaveAttribute('aria-valuenow', '10')
    expect(screen.getByText('1 reset available')).toBeInTheDocument()
  })

  it('renders primary, weekly, scoped-weekly, and credits meters for a Claude-style app', () => {
    render(
      <Meters
        app="claude"
        quota={{
          primary: { utilization: 63, resetsAt: null },
          secondary: { utilization: 21, resetsAt: null },
          scopedWeekly: [{ utilization: 8, resetsAt: null, label: 'Fable' }],
          spend: { usedMinor: 100, limitMinor: 1000, currency: 'USD', exponent: 2, percent: 10 },
        }}
      />,
    )
    expect(screen.getAllByRole('progressbar')).toHaveLength(4)
    expect(screen.getByRole('progressbar', { name: '5-hour window' })).toHaveAttribute('aria-valuenow', '63')
    expect(screen.getByRole('progressbar', { name: 'Weekly · Fable' })).toHaveAttribute('aria-valuenow', '8')
    expect(screen.getByRole('progressbar', { name: 'Usage credits' })).toHaveAttribute('aria-valuenow', '10')
  })

  it('renders the primary and weekly placeholders for a null quota, with no scoped weekly or credits', () => {
    render(<Meters app="claude" quota={null} />)
    const bars = screen.getAllByRole('progressbar')
    expect(bars).toHaveLength(2)
    for (const bar of bars) {
      expect(bar).not.toHaveAttribute('aria-valuenow')
    }
  })
})

describe('codexWindowLabel', () => {
  it.each([
    [10080, 'Weekly', 'W'],
    [300, '5-hour window', '5h'],
    [60, '1-hour window', '1h'],
    [45, '45-minute window', '45m'],
    [null, 'Usage window', 'Usage'],
    [undefined, 'Usage window', 'Usage'],
  ] as const)('labels a %s-minute window', (minutes, label, shortLabel) => {
    expect(codexWindowLabel(minutes)).toEqual({ label, shortLabel })
  })
})

describe('codexMeterRows', () => {
  it('skips a slot the payload leaves empty', () => {
    const rows = codexMeterRows({
      primary: { utilization: 10, resetsAt: null },
      secondary: null,
      scopedWeekly: [],
    })
    expect(rows).toHaveLength(1)
    expect(rows[0].slot).toBe('primary')
  })

  it('returns no rows when quota is null', () => {
    expect(codexMeterRows(null)).toEqual([])
  })

  it('marks a weekly (10080-minute) window for daily segments and passes its pace duration', () => {
    const rows = codexMeterRows({
      primary: { utilization: 10, resetsAt: null, windowDurationMins: 10080 },
      secondary: null,
      scopedWeekly: [],
    })
    expect(rows[0]).toMatchObject({ label: 'Weekly', showDailySegments: true, paceWindowMins: 10080 })
  })

  it('renders both slots in order when both are present', () => {
    const rows = codexMeterRows({
      primary: { utilization: 10, resetsAt: null, windowDurationMins: 300 },
      secondary: { utilization: 5, resetsAt: null, windowDurationMins: 10080 },
      scopedWeekly: [],
    })
    expect(rows.map((row) => row.slot)).toEqual(['primary', 'secondary'])
    expect(rows[1]).toMatchObject({ label: 'Weekly', showDailySegments: true })
  })
})

describe('visibleScopedWeekly', () => {
  it('returns no rows when the app does not report scoped weekly quotas', () => {
    expect(visibleScopedWeekly(false, [{ utilization: 10, resetsAt: null }])).toEqual([])
  })

  it('returns no rows when scopedWeekly is undefined', () => {
    expect(visibleScopedWeekly(true, undefined)).toEqual([])
  })

  it('drops a row the user has not touched this window', () => {
    const untouched = { utilization: 0, resetsAt: null }
    const touched = { utilization: 5, resetsAt: null }
    expect(visibleScopedWeekly(true, [untouched, touched])).toEqual([touched])
  })

  it('keeps a row with unknown (null) utilization', () => {
    const unknown = { utilization: null, resetsAt: null }
    expect(visibleScopedWeekly(true, [unknown])).toEqual([unknown])
  })
})

describe('toggleUsageDisplay', () => {
  it('flips used to remaining and back', () => {
    expect(toggleUsageDisplay('used')).toBe('remaining')
    expect(toggleUsageDisplay('remaining')).toBe('used')
  })
})

describe('codexDisplayToggleCopy', () => {
  it('describes switching away from used', () => {
    expect(codexDisplayToggleCopy('used')).toEqual({
      buttonLabel: 'Used',
      ariaLabel: 'Show remaining quota',
      tooltip: 'Switch to remaining quota',
    })
  })

  it('describes switching away from remaining', () => {
    expect(codexDisplayToggleCopy('remaining')).toEqual({
      buttonLabel: 'Remaining',
      ariaLabel: 'Show used quota',
      tooltip: 'Switch to used quota',
    })
  })
})

describe('usedPercentFromUtilization', () => {
  it('keeps null utilization as null', () => {
    expect(usedPercentFromUtilization(null)).toBeNull()
  })

  it('rounds to the nearest whole percent', () => {
    expect(usedPercentFromUtilization(42.6)).toBe(43)
  })

  it('keeps values over 100 for an over-limit account', () => {
    expect(usedPercentFromUtilization(142)).toBe(142)
  })
})

describe('displayPercent', () => {
  it('passes a used-percent through unchanged for the used display', () => {
    expect(displayPercent(30, 'used')).toBe(30)
  })

  it('inverts a used-percent for the remaining display', () => {
    expect(displayPercent(30, 'remaining')).toBe(70)
  })

  it('floors remaining at 0 for an over-limit account', () => {
    expect(displayPercent(142, 'remaining')).toBe(0)
  })

  it('keeps null utilization as null in either display', () => {
    expect(displayPercent(null, 'used')).toBeNull()
    expect(displayPercent(null, 'remaining')).toBeNull()
  })
})

describe('meterFillPercent', () => {
  it('fills to 0 when there is no data', () => {
    expect(meterFillPercent(null)).toBe(0)
  })

  it('passes a normal percent through', () => {
    expect(meterFillPercent(55)).toBe(55)
  })

  it('caps an over-limit percent at 100', () => {
    expect(meterFillPercent(142)).toBe(100)
  })

  it('floors a negative percent at 0', () => {
    expect(meterFillPercent(-5)).toBe(0)
  })
})

describe('meterTone', () => {
  it('is muted with no data', () => {
    expect(meterTone(null)).toBe('muted')
  })

  it('is ok under 50%', () => {
    expect(meterTone(49)).toBe('ok')
  })

  it('is warn between 50% and 80%', () => {
    expect(meterTone(50)).toBe('warn')
    expect(meterTone(79)).toBe('warn')
  })

  it('is crit at 80% and above', () => {
    expect(meterTone(80)).toBe('crit')
    expect(meterTone(142)).toBe('crit')
  })
})

describe('displayPacePercent', () => {
  it('keeps null pace as null', () => {
    expect(displayPacePercent(null, 'used')).toBeNull()
  })

  it('passes pace through unchanged for the used display', () => {
    expect(displayPacePercent(40, 'used')).toBe(40)
  })

  it('mirrors pace across the bar for the remaining display', () => {
    expect(displayPacePercent(40, 'remaining')).toBe(60)
  })
})
