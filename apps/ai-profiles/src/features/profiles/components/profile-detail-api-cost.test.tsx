import type { ApiCost } from '@/lib/types'

import { fireEvent, screen, waitFor } from '@testing-library/react'
import { addDays, format, parseISO } from 'date-fns'
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
    // The 30-day window, up to Oct 15.
    days: [
      ...Array.from({ length: 29 }, (_, index) => ({
        date: format(addDays(parseISO('2026-09-16'), index), 'yyyy-MM-dd'),
        totalUsd: 0,
        byModel: [],
      })),
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
    coverage: 'complete',
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

  it('says the use is unpriced, not $0, when none of it has a price', async () => {
    vi.mocked(getProfileApiCost).mockResolvedValue(
      makeCost({
        totalUsd: 0,
        projectedUsd: 0,
        byModel: [],
        projects: [],
        unpriced: [{ model: 'Opus 9', missing: null, tokens: 1_200 }],
        coverage: 'unpriced',
      }),
    )
    renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
    const row = await screen.findByRole('button', { name: /Estimated API cost/ })
    expect(row.textContent).toContain('Unpriced')
    expect(row.textContent).not.toContain('$')

    fireEvent.click(row)
    expect(await screen.findByText(/None of the use over the 24 days/)).toBeTruthy()
    expect(screen.getByText(/No price for Opus 9/)).toBeTruthy()
    expect(screen.queryByRole('img', { name: /Cost per day/ })).toBeNull()
  })

  it('labels a total that leaves use without a price out as partial', async () => {
    vi.mocked(getProfileApiCost).mockResolvedValue(
      makeCost({ unpriced: [{ model: 'Opus 9', missing: null, tokens: 1_200 }], coverage: 'partial' }),
    )
    renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
    const row = await screen.findByRole('button', { name: /Estimated API cost/ })
    expect(row.textContent).toContain('$559')
    expect(row.textContent).toContain('partial')

    fireEvent.click(row)
    expect(await screen.findByText(/No price for Opus 9/)).toBeTruthy()
    expect(screen.getByText(/Partial: use without a price is left out/)).toBeTruthy()
  })

  it('leaves a complete total unlabelled', async () => {
    renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
    const row = await screen.findByRole('button', { name: /Estimated API cost/ })
    expect(row.textContent).not.toMatch(/partial|unpriced/i)
  })

  it('shows nothing for a profile without transcripts', async () => {
    vi.mocked(getProfileApiCost).mockResolvedValue(makeCost({ coveredDays: 0, totalUsd: 0, projectedUsd: null }))
    renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
    await waitFor(() => expect(screen.queryByText('Estimated API cost')).toBeNull())
  })

  it('holds its place while the transcripts are read', () => {
    vi.mocked(getProfileApiCost).mockReturnValue(new Promise(() => {}))
    renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
    expect(screen.getByText('Estimated API cost')).toBeTruthy()
    expect(screen.queryByRole('button', { name: /Estimated API cost/ })).toBeNull()
  })

  it("says when the transcripts couldn't be read", async () => {
    vi.mocked(getProfileApiCost).mockRejectedValue(new Error('profile not found'))
    renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
    expect(await screen.findByText("Couldn't read the transcripts")).toBeTruthy()
  })

  it('estimates Codex use at OpenAI API prices, without ChatGPT chats', async () => {
    renderWithQuery(<ProfileDetailApiCost app="codex" profileId="p1" />)
    fireEvent.click(await screen.findByRole('button', { name: /Estimated API cost/ }))

    expect(await screen.findByText(/Codex use on this profile would cost at OpenAI API prices/)).toBeTruthy()
    expect(screen.getByText(/From this profile's Codex transcripts/)).toBeTruthy()
    expect(screen.getByText(/Only Codex use is included.*ChatGPT chats keep no token counts/)).toBeTruthy()
    expect(screen.getByText('Cache writes')).toBeTruthy()
    expect(screen.queryByText(/Claude|claude\.ai/)).toBeNull()
  })

  it('estimates Claude Code use at Claude API prices, without claude.ai chats', async () => {
    renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
    fireEvent.click(await screen.findByRole('button', { name: /Estimated API cost/ }))

    expect(await screen.findByText(/Claude Code use on this profile would cost at Claude API prices/)).toBeTruthy()
    expect(screen.getByText(/on claude\.ai keep no token counts, so they aren't included/)).toBeTruthy()
    expect(screen.queryByText(/ChatGPT|OpenAI/)).toBeNull()
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
    expect(screen.getByText(/LiteLLM's price list as of Oct 15, 2026/)).toBeTruthy()
    expect(screen.queryByRole('textbox')).toBeNull()
    expect(screen.queryByText(/your plan|what you pay/i)).toBeNull()
  })

  it('tells projects of the same name apart by their paths', async () => {
    vi.mocked(getProfileApiCost).mockResolvedValue(
      makeCost({
        projects: [
          { path: '/Users/ada/client-a/api', usd: 3 },
          { path: '/Users/ada/client-b/api', usd: 2 },
          { path: '/Users/ada/web', usd: 1 },
        ],
      }),
    )
    renderWithQuery(<ProfileDetailApiCost app="claude" profileId="p1" />)
    fireEvent.click(await screen.findByRole('button', { name: /Estimated API cost/ }))

    expect(await screen.findByText('~/client-a/api')).toBeTruthy()
    expect(screen.getByText('~/client-b/api')).toBeTruthy()
    expect(screen.getByText('web')).toBeTruthy()
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
    expect(summaryText(makeCost({ coveredDays: 24, projectedUsd: 702 }))).toBe(
      'Over the 24 days your transcripts cover. On pace for $702 over 30 days.',
    )
  })

  it('gives no pace for under a week', () => {
    expect(summaryText(makeCost({ coveredDays: 3, projectedUsd: null }))).toBe(
      'Over the 3 days your transcripts cover.',
    )
  })

  it('says the window alone when the transcripts cover all of it', () => {
    expect(summaryText(makeCost({ coveredDays: 30, projectedUsd: null }))).toBe('Over the last 30 days.')
  })

  it('says when the total is partial or nothing has a price', () => {
    expect(summaryText(makeCost({ coveredDays: 30, projectedUsd: null, coverage: 'partial' }))).toBe(
      'Over the last 30 days. Partial: use without a price is left out.',
    )
    expect(summaryText(makeCost({ coveredDays: 3, projectedUsd: null, coverage: 'unpriced' }))).toBe(
      'None of the use over the 3 days your transcripts cover has a price.',
    )
  })
})

describe('formatting', () => {
  it('drops the cents from $100 up', () => {
    expect(formatUsd(4.5)).toBe('$4.50')
    expect(formatUsd(20)).toBe('$20')
    expect(formatUsd(1234.56)).toBe('$1,235')
  })

  it('shows an amount too small for cents as under a cent, not as $0.00', () => {
    expect(formatUsd(0.004)).toBe('<$0.01')
    expect(formatUsd(0.006)).toBe('$0.01')
    expect(formatUsd(0)).toBe('$0')
  })

  it('shows a share too small to round up as under 1%', () => {
    expect(formatShare(0.0001)).toBe('<1%')
    expect(formatShare(0)).toBe('0%')
    expect(formatShare(0.426)).toBe('43%')
  })
})

describe('kindRows', () => {
  it('lists the kinds with any cost, most expensive first', () => {
    const rows = kindRows(
      { input: 1, output: 3, cacheWrite5m: 0.5, cacheWrite1h: 2, cacheRead: 0, webSearch: 0 },
      'Cache writes, 5 min',
    )
    expect(rows.map((row) => row.label)).toEqual(['Output', 'Cache writes, 1 hour', 'Input', 'Cache writes, 5 min'])
  })
})

describe('unpricedNote', () => {
  it('names the models, what the price list lacks for each and how many tokens went uncounted', () => {
    expect(
      unpricedNote([
        { model: 'Opus 9', missing: null, tokens: 1_200_000 },
        { model: 'Opus 5.5', missing: 'fast-mode rates', tokens: 3_400 },
      ]),
    ).toBe(
      "No price for Opus 9 (1,200,000 tokens). No fast-mode rates for Opus 5.5 (3,400 tokens). They aren't counted. LiteLLM's price list may add the models it lacks; it's checked daily.",
    )
  })

  it("doesn't promise rates the price list may never have", () => {
    expect(unpricedNote([{ model: 'Opus 5.5', missing: 'fast-mode rates', tokens: 3_400 }])).toBe(
      "No fast-mode rates for Opus 5.5 (3,400 tokens). They aren't counted.",
    )
  })
})
