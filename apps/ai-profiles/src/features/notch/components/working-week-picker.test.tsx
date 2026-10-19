import type { AppState } from '@/lib/types'

import { fireEvent, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { loadAppState, updateAppState } from '@/lib/commands'
import { renderWithQuery } from '@/test/render-with-query'

import { WorkingWeekPicker } from './working-week-picker'

vi.mock('@/lib/commands', () => ({ loadAppState: vi.fn(), updateAppState: vi.fn() }))

function appState(overrides: Partial<AppState> = {}): AppState {
  return {
    welcomeShown: true,
    migrationDismissedAt: null,
    pathBannerDismissedAt: null,
    themeMode: 'system',
    selectedEntryId: null,
    dockIconAcknowledgedAt: null,
    defaultProfileNames: {},
    dismissedRepairSessions: {},
    notchHudOff: false,
    fiveDayWeekProfiles: [],
    ...overrides,
  }
}

beforeEach(() => {
  vi.mocked(updateAppState).mockImplementation(async (patch) =>
    appState({ fiveDayWeekProfiles: patch.workingWeek?.days === 5 ? ['p1'] : [] }),
  )
})

describe('WorkingWeekPicker', () => {
  it('starts on seven days and gives the profile a five-day week', async () => {
    vi.mocked(loadAppState).mockResolvedValue(appState())
    renderWithQuery(<WorkingWeekPicker profileId="p1" />)

    expect(await screen.findByRole('radio', { name: 'Seven-day week' })).toHaveAttribute('aria-checked', 'true')
    fireEvent.click(screen.getByRole('radio', { name: 'Five-day week, Monday to Friday' }))
    await waitFor(() => expect(updateAppState).toHaveBeenCalledWith({ workingWeek: { profileId: 'p1', days: 5 } }))
  })

  it('is absent while the notch HUD is off', async () => {
    vi.mocked(loadAppState).mockResolvedValue(appState({ notchHudOff: true }))
    renderWithQuery(<WorkingWeekPicker profileId="p1" />)

    await waitFor(() => expect(loadAppState).toHaveBeenCalled())
    expect(screen.queryByRole('radiogroup')).not.toBeInTheDocument()
  })
})
