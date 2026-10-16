import type { ApiCost } from '@/lib/types'

import { fireEvent, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { getProfileApiCost } from '@/lib/commands'
import { renderWithQuery } from '@/test/render-with-query'

import {
  formatShare,
  formatUsd,
  kindRows,
  ProfileDetailApiCost,
  summaryText,
  unpricedNote,
} from './profile-detail-api-cost'

vi.mock('@/lib/commands', () => ({
  getProfileApiCost: vi.fn(),
}))

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
    ...overrides,
  }
}

beforeEach(() => {
  vi.mocked(getProfileApiCost).mockReset()
  vi.mocked(getProfileApiCost).mockResolvedValue(makeCost())
})

describe('ProfileDetailApiCost', () => {
  it('estimates what the use would cost, without comparing it against a plan', async () => {
    renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
    const row = await screen.findByRole('button', { name: /Estimated API cost/ })
    expect(row.textContent).toContain('last 24d')
    expect(row.textContent).toContain('$559')
    expect(row.textContent).not.toMatch(/×|plan|save/i)
    expect(getProfileApiCost).toHaveBeenCalledWith('p1')
  })

  it('shows nothing for a profile without transcripts', async () => {
    vi.mocked(getProfileApiCost).mockResolvedValue(makeCost({ coveredDays: 0, totalUsd: 0, projectedUsd: null }))
    renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
    await waitFor(() => expect(getProfileApiCost).toHaveBeenCalled())
    expect(screen.queryByRole('button', { name: /Estimated API cost/ })).toBeNull()
  })

  it('never asks for an app without transcripts to price', () => {
    renderWithQuery(<ProfileDetailApiCost app="codex" profileId="p1" />)
    expect(getProfileApiCost).not.toHaveBeenCalled()
    expect(screen.queryByRole('button', { name: /Estimated API cost/ })).toBeNull()
  })

  it('opens the breakdown, which asks for no plan price', async () => {
    renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
    fireEvent.click(await screen.findByRole('button', { name: /Estimated API cost/ }))

    expect(await screen.findByText('Cache reads')).toBeTruthy()
    expect(screen.getByText('app')).toBeTruthy()
    expect(screen.getByText('~')).toBeTruthy()
    expect(screen.getByText('Unknown folder')).toBeTruthy()
    expect(screen.getByText('peak $12.50/day')).toBeTruthy()
    expect(screen.getByText(/not what you were billed/)).toBeTruthy()
    expect(screen.queryByRole('textbox')).toBeNull()
    expect(screen.queryByText(/your plan|what you pay/i)).toBeNull()
  })
})

it('charts only the days the transcripts cover', async () => {
  vi.mocked(getProfileApiCost).mockResolvedValue(makeCost({ coveredDays: 1 }))
  renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
  fireEvent.click(await screen.findByRole('button', { name: /Estimated API cost/ }))
  expect(await screen.findByRole('img', { name: 'Cost per day over the last 1 day' })).toBeTruthy()
})

describe('summaryText', () => {
  it('says how far the transcripts go back and the pace when they cover only part of the window', () => {
    expect(summaryText({ coveredDays: 24, projectedUsd: 702 })).toBe(
      'Over the 24 days your transcripts cover. On pace for $702 over 30 days.',
    )
  })

  it('says the window alone when the transcripts cover all of it', () => {
    expect(summaryText({ coveredDays: 30, projectedUsd: null })).toBe('Over the last 30 days.')
  })
})

describe('formatting', () => {
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
