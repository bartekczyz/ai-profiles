import type { ReactNode } from 'react'
import type { ProfileUsage } from '@/lib/types'

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { useProfileUsage } from '../api/use-profile-usage'
import { ProfileDetailUsageCard } from './profile-detail-usage-card'

// vi.mock is hoisted above all imports — the import above resolves to the
// mocked module at test time.
vi.mock('../api/use-profile-usage', () => ({
  useProfileUsage: vi.fn(),
  refetchIntervalMs: 5 * 60 * 1000,
}))

function makeUsage(overrides: Partial<ProfileUsage> = {}): ProfileUsage {
  return {
    quota: {
      // Utilization is a 0..=100 percentage — matches Anthropic's response.
      fiveHour: { utilization: 63, resetsAt: null },
      sevenDay: { utilization: 21, resetsAt: null },
      sevenDaySonnet: { utilization: 8, resetsAt: null },
    },
    quotaError: null,
    fetchedAt: '2099-01-01T00:00:00Z',
    ...overrides,
  }
}

function renderWithQuery(children: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(<QueryClientProvider client={client}>{children}</QueryClientProvider>)
}

describe('ProfileDetailUsageCard', () => {
  it('renders three progressbars with the right aria-valuenow', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage(),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard profileId="p1" cliEnabled={true} />)
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
          fiveHour: { utilization: null, resetsAt: null },
          sevenDay: { utilization: 21, resetsAt: null },
          sevenDaySonnet: { utilization: 8, resetsAt: null },
        },
      }),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard profileId="p1" cliEnabled={true} />)
    const bars = screen.getAllByRole('progressbar')
    expect(bars[0]).not.toHaveAttribute('aria-valuenow')
  })

  it('hides the meters when quotaError is no_credentials', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage({ quota: null, quotaError: 'no_credentials' }),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard profileId="p1" cliEnabled={true} />)
    expect(screen.queryByRole('progressbar')).toBeNull()
  })

  it.each([
    ['unauthorized', /token refresh needed/i],
    ['rate_limited', /rate limited/i],
    ['network', /couldn't reach anthropic/i],
    ['unknown', /couldn't load usage stats/i],
  ] as const)('shows an explicit message and no meters when quotaError is %s', (quotaError, expected) => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage({ quota: null, quotaError }),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard profileId="p1" cliEnabled={true} />)
    expect(screen.queryByRole('progressbar')).toBeNull()
    expect(screen.getByText(expected)).toBeInTheDocument()
  })

  it('shows the unknown-error message when quota is null with no quotaError (defensive)', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage({ quota: null, quotaError: null }),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard profileId="p1" cliEnabled={true} />)
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
    renderWithQuery(<ProfileDetailUsageCard profileId="p1" cliEnabled={true} />)
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
    const { container } = renderWithQuery(<ProfileDetailUsageCard profileId="p1" cliEnabled={false} />)
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
    renderWithQuery(<ProfileDetailUsageCard profileId="p1" cliEnabled={true} />)
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
    renderWithQuery(<ProfileDetailUsageCard profileId="p1" cliEnabled={true} />)
    expect(screen.getByText(/refreshing/)).toBeInTheDocument()
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
        <ProfileDetailUsageCard profileId="p1" cliEnabled={true} />
      </QueryClientProvider>,
    )
    expect(screen.getByText(/couldn't display usage stats/i)).toBeInTheDocument()

    rerender(
      <QueryClientProvider client={client}>
        <ProfileDetailUsageCard profileId="p2" cliEnabled={true} />
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

    renderWithQuery(<ProfileDetailUsageCard profileId="p1" cliEnabled={true} />)
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
          fiveHour: { utilization: 40, resetsAt: '2099-06-15T14:30:00Z' },
          sevenDay: { utilization: 10, resetsAt: '2099-06-22T09:00:00Z' },
          sevenDaySonnet: { utilization: 5, resetsAt: null },
        },
      }),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard profileId="p1" cliEnabled={true} />)
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
          fiveHour: { utilization: 40, resetsAt: null },
          sevenDay: { utilization: 10, resetsAt: null },
          sevenDaySonnet: { utilization: 0, resetsAt: null },
        },
      }),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard profileId="p1" cliEnabled={true} />)
    // Only 5-hour and weekly bars render; Weekly Sonnet is suppressed.
    expect(screen.getAllByRole('progressbar')).toHaveLength(2)
  })

  it('keeps the Weekly Sonnet row visible when its utilization is unknown (null)', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage({
        quota: {
          fiveHour: { utilization: 40, resetsAt: null },
          sevenDay: { utilization: 10, resetsAt: null },
          sevenDaySonnet: { utilization: null, resetsAt: null },
        },
      }),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard profileId="p1" cliEnabled={true} />)
    // Unknown utilization is not the same as zero — we still surface
    // the row (with a "—" placeholder) so the user can tell data is
    // missing rather than confused with "no usage this week".
    expect(screen.getAllByRole('progressbar')).toHaveLength(3)
  })

  it('omits the reset tooltip when resetsAt is null', () => {
    ;(useProfileUsage as ReturnType<typeof vi.fn>).mockReturnValue({
      data: makeUsage({
        quota: {
          fiveHour: { utilization: 40, resetsAt: null },
          sevenDay: { utilization: 10, resetsAt: null },
          sevenDaySonnet: { utilization: 5, resetsAt: null },
        },
      }),
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    })
    renderWithQuery(<ProfileDetailUsageCard profileId="p1" cliEnabled={true} />)
    const datetimePattern = /^[A-Z][a-z]{2} \d{1,2} [A-Z][a-z]{2}, \d{2}:\d{2}$/
    const tooltips = screen.queryAllByRole('tooltip')
    const datetimeTooltips = tooltips.filter((node) => datetimePattern.test((node.textContent ?? '').trim()))
    expect(datetimeTooltips).toHaveLength(0)
  })
})
