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
})
