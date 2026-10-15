import type { ApiCost, AppState } from '@/lib/types'

import { fireEvent, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { getProfileApiCost, loadAppState, updateAppState } from '@/lib/commands'
import { renderWithQuery } from '@/test/render-with-query'

import {
  formatMultiple,
  formatShare,
  formatUsd,
  kindRows,
  ProfileDetailApiCost,
  parseUsdToCents,
  planMultiple,
  summaryText,
  unpricedNote,
} from './profile-detail-api-cost'

vi.mock('@/lib/commands', () => ({
  getProfileApiCost: vi.fn(),
  loadAppState: vi.fn(),
  updateAppState: vi.fn(),
}))

function appState(planPrices: AppState['planPrices'] = {}): AppState {
  return {
    welcomeShown: true,
    migrationDismissedAt: null,
    pathBannerDismissedAt: null,
    themeMode: 'system',
    selectedEntryId: null,
    dockIconAcknowledgedAt: null,
    defaultProfileNames: {},
    dismissedRepairSessions: {},
    planPrices,
  }
}

function makeCost(overrides: Partial<ApiCost> = {}): ApiCost {
  return {
    totalUsd: 559.07,
    projectedUsd: 698.84,
    coveredDays: 24,
    days: [
      { date: '2026-10-14', totalUsd: 0, byModel: [] },
      {
        date: '2026-10-15',
        totalUsd: 12.5,
        byModel: [
          { model: 'Opus 5.5', usd: 10 },
          { model: 'Sonnet 5.5', usd: 2.5 },
        ],
      },
    ],
    byKind: { input: 0.07, output: 133.31, cacheWrite5m: 18.93, cacheWrite1h: 168.04, cacheRead: 238.72, webSearch: 0 },
    byModel: [
      { model: 'Opus 5.5', usd: 549.49 },
      { model: 'Sonnet 5.5', usd: 9.58 },
    ],
    projects: [
      { path: '/Users/ada/work/app', usd: 549.63 },
      { path: '/Users/ada', usd: 5.2 },
      { path: null, usd: 4.24 },
    ],
    unpriced: [],
    pricesAsOf: '2026-10-15',
    plan: 'Pro',
    planListPriceCents: 2_000,
    ...overrides,
  }
}

beforeEach(() => {
  vi.mocked(getProfileApiCost).mockReset()
  vi.mocked(getProfileApiCost).mockResolvedValue(makeCost())
  vi.mocked(loadAppState).mockResolvedValue(appState())
  vi.mocked(updateAppState).mockReset()
  vi.mocked(updateAppState).mockResolvedValue(appState())
})

describe('ProfileDetailApiCost', () => {
  it('says what the use would cost and how many times the plan that is, at the pace so far', async () => {
    renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
    const row = await screen.findByRole('button', { name: /API equivalent/ })
    expect(row.textContent).toContain('last 24d')
    expect(row.textContent).toContain('$559')
    expect(row.textContent).toContain('on pace for 35× your Pro plan')
    expect(getProfileApiCost).toHaveBeenCalledWith('p1')
  })

  it('compares against the price the user set in place of the list price', async () => {
    vi.mocked(loadAppState).mockResolvedValue(appState({ p1: 10_000 }))
    renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
    const row = await screen.findByRole('button', { name: /API equivalent/ })
    expect(row.textContent).toContain('on pace for 7.0× your Pro plan')
  })

  it('leaves the multiple out without a price to compare against', async () => {
    vi.mocked(getProfileApiCost).mockResolvedValue(makeCost({ plan: 'Team', planListPriceCents: null }))
    renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
    const row = await screen.findByRole('button', { name: /API equivalent/ })
    expect(row.textContent).not.toContain('×')
  })

  it('shows nothing for a profile without transcripts', async () => {
    vi.mocked(getProfileApiCost).mockResolvedValue(makeCost({ coveredDays: 0, totalUsd: 0, projectedUsd: null }))
    renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
    await waitFor(() => expect(getProfileApiCost).toHaveBeenCalled())
    expect(screen.queryByRole('button', { name: /API equivalent/ })).toBeNull()
  })

  it('never asks for an app without transcripts to price', () => {
    renderWithQuery(<ProfileDetailApiCost app="codex" profileId="p1" />)
    expect(getProfileApiCost).not.toHaveBeenCalled()
    expect(screen.queryByRole('button', { name: /API equivalent/ })).toBeNull()
  })

  it('opens the breakdown and keeps the price the user enters', async () => {
    renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
    fireEvent.click(await screen.findByRole('button', { name: /API equivalent/ }))

    expect(await screen.findByText('Cache reads')).toBeTruthy()
    expect(screen.getByText('app')).toBeTruthy()
    expect(screen.getByText('~')).toBeTruthy()
    expect(screen.getByText('Unknown folder')).toBeTruthy()
    expect(screen.getByText('Pro plan: $20/mo')).toBeTruthy()
    expect(screen.getByText('peak $12.50/day')).toBeTruthy()

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }))
    fireEvent.change(screen.getByLabelText('You pay a month, USD'), { target: { value: '$17' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(vi.mocked(updateAppState).mock.calls[0]?.[0]).toEqual({ planPrice: { profileId: 'p1', cents: 1_700 } }),
    )
  })
})

it('charts only the days the transcripts cover', async () => {
  vi.mocked(getProfileApiCost).mockResolvedValue(makeCost({ coveredDays: 1 }))
  renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
  fireEvent.click(await screen.findByRole('button', { name: /API equivalent/ }))
  expect(await screen.findByRole('img', { name: 'Cost per day over the last 1 day' })).toBeTruthy()
})

describe('summaryText', () => {
  it('says how far the transcripts go back and the pace when they cover only part of the window', () => {
    expect(summaryText({ coveredDays: 24, projectedUsd: 702 }, 35.1)).toBe(
      'Over the 24 days your transcripts cover. On pace for $702 over 30 days, 35× what you pay.',
    )
  })

  it('says the window alone when the transcripts cover all of it', () => {
    expect(summaryText({ coveredDays: 30, projectedUsd: null }, null)).toBe('Over the last 30 days.')
  })
})

describe('planMultiple', () => {
  it('compares the projection when there is one, else the total', () => {
    expect(planMultiple({ totalUsd: 30, projectedUsd: 60 }, 2_000)).toBe(3)
    expect(planMultiple({ totalUsd: 30, projectedUsd: null }, 2_000)).toBe(1.5)
  })

  it('is null without a price', () => {
    expect(planMultiple({ totalUsd: 30, projectedUsd: null }, null)).toBeNull()
    expect(planMultiple({ totalUsd: 30, projectedUsd: null }, 0)).toBeNull()
  })
})

describe('formatting', () => {
  it('keeps one decimal below 10× and rounds above', () => {
    expect(formatMultiple(4.26)).toBe('4.3×')
    expect(formatMultiple(34.94)).toBe('35×')
  })

  it('drops the cents from $100 up', () => {
    expect(formatUsd(4.5)).toBe('$4.50')
    expect(formatUsd(20)).toBe('$20')
    expect(formatUsd(1234.56)).toBe('$1,235')
  })

  it('shows a share too small to round up as under 1%', () => {
    expect(formatShare(0.0001)).toBe('<1%')
    expect(formatShare(0)).toBe('0%')
    expect(formatShare(0.426)).toBe('43%')
  })
})

describe('parseUsdToCents', () => {
  it('reads dollars, with or without a dollar sign', () => {
    expect(parseUsdToCents('20')).toBe(2_000)
    expect(parseUsdToCents(' $16.67 ')).toBe(1_667)
  })

  it('refuses anything that is not a positive amount', () => {
    expect(parseUsdToCents('')).toBeNull()
    expect(parseUsdToCents('0')).toBeNull()
    expect(parseUsdToCents('-5')).toBeNull()
    expect(parseUsdToCents('20 PLN')).toBeNull()
    expect(parseUsdToCents('1.234')).toBeNull()
  })
})

describe('kindRows', () => {
  it('lists the kinds with any cost, most expensive first', () => {
    const rows = kindRows({ input: 1, output: 3, cacheWrite5m: 0, cacheWrite1h: 2, cacheRead: 0, webSearch: 0 })
    expect(rows.map((row) => row.label)).toEqual(['Output', 'Cache writes, 1 hour', 'Input'])
  })
})

describe('unpricedNote', () => {
  it('names the models and how many tokens went uncounted', () => {
    expect(unpricedNote([{ model: 'claude-opus-9', tokens: 1_200_000 }])).toBe(
      "No price for claude-opus-9 (1,200,000 tokens), so they aren't counted. A newer ai-profiles may know it.",
    )
  })
})
