import type { AppState, Profile, SidebarEntry } from '@/lib/types'
import type { NotchState } from '../api/notch-ipc'

import { fireEvent, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { fetchUsableUsage } from '@/features/profiles/api/use-profile-usage'
import { useSidebarEntries } from '@/features/profiles/api/use-sidebar-entries'
import { useAppState } from '@/lib/app-state/use-app-state'
import { renderWithQuery } from '@/test/render-with-query'

import { getNotchState, onNotchState, openProfileFromNotch } from '../api/notch-ipc'
import { NotchHud } from './notch-hud'

vi.mock('../api/notch-ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../api/notch-ipc')>()),
  getNotchState: vi.fn(),
  onNotchState: vi.fn(),
  openProfileFromNotch: vi.fn(),
  setNotchContentHeight: vi.fn(),
  setNotchWingWidth: vi.fn(),
}))
vi.mock('@/features/profiles/api/use-sidebar-entries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/features/profiles/api/use-sidebar-entries')>()),
  useSidebarEntries: vi.fn(),
}))
vi.mock('@/features/profiles/api/use-profile-usage', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/features/profiles/api/use-profile-usage')>()),
  fetchUsableUsage: vi.fn(),
}))
vi.mock('@/lib/app-state/use-app-state', () => ({ useAppState: vi.fn() }))

const expanded: NotchState = {
  phase: 'expanded',
  notch: 189,
  height: 32,
  wing: 30,
  fillet: 19,
  bodyWidth: 320,
  bodyMaxHeight: 1000,
}

function managed(id: string, name: string, cli = true): SidebarEntry {
  const profile: Profile = {
    id,
    app: 'claude',
    name,
    slug: id,
    color: '#7c9a6e',
    createdAt: '2026-01-01T00:00:00Z',
    surfaces: { gui: true, cli },
    distinctDockIcon: false,
    lastUsedAt: null,
  }
  return { kind: 'managed', profile }
}

function appState(): AppState {
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
  }
}

function given({ entries }: { entries: Array<SidebarEntry> }) {
  vi.mocked(useSidebarEntries).mockReturnValue(entries)
  vi.mocked(useAppState).mockReturnValue({ state: appState(), update: vi.fn(), refresh: vi.fn() })
}

beforeEach(() => {
  vi.mocked(getNotchState).mockResolvedValue(expanded)
  vi.mocked(onNotchState).mockResolvedValue(() => undefined)
  vi.mocked(fetchUsableUsage).mockImplementation(async (id) => ({
    quota: {
      primary: { utilization: 12, resetsAt: new Date(Date.now() + 3 * 3600_000).toISOString() },
      secondary: {
        utilization: id === 'work' ? 32 : 85,
        resetsAt: new Date(Date.now() + (4 * 24 + 23) * 3600_000 + 30_000).toISOString(),
      },
      scopedWeekly: [],
    },
    quotaError: null,
    fetchedAt: new Date().toISOString(),
  }))
})

describe('NotchHud', () => {
  it('shows each profile’s 5-hour bar over its weekly bar, with their resets', async () => {
    given({ entries: [managed('work', 'Work'), managed('home', 'Home')] })
    renderWithQuery(<NotchHud />)

    expect(await screen.findByText('Home')).toBeInTheDocument()
    const work = screen.getByText('Work').closest('button')
    const captions = () =>
      Array.from(work?.querySelectorAll('.notch-limit-caption') ?? []).map((caption) => caption.textContent)
    await vi.waitFor(() => expect(captions()).toEqual(['5h · resets in 3h 0m12%', 'Week · resets in 4d 23h32%']))
  })

  it('sums every profile up in the folded notch, split either side of the camera', async () => {
    given({ entries: [managed('work', 'Work'), managed('home', 'Home'), managed('play', 'Play')] })
    renderWithQuery(<NotchHud />)
    await screen.findByText('Work')

    const chips = (side: string) =>
      Array.from(document.querySelectorAll(`.notch-wing-${side} .notch-chip`)).map((chip) => [
        chip.getAttribute('title'),
        chip.textContent,
      ])
    await vi.waitFor(() =>
      expect(chips('left')).toEqual([
        ['Work', '32%'],
        ['Home', '85%'],
      ]),
    )
    expect(chips('right')).toEqual([['Play', '85%']])
    expect(document.querySelector('[title="Home"] .notch-chip-figure')).toHaveAttribute('data-level', 'warn')
  })

  it('leaves out profiles without a CLI, which have no usage to show', async () => {
    given({ entries: [managed('work', 'Work'), managed('gui', 'Desktop only', false)] })
    renderWithQuery(<NotchHud />)

    expect(await screen.findByText('Work')).toBeInTheDocument()
    expect(screen.queryByText('Desktop only')).not.toBeInTheDocument()
  })

  it('opens the clicked profile in the main window', async () => {
    given({ entries: [managed('work', 'Work')] })
    renderWithQuery(<NotchHud />)

    fireEvent.click(await screen.findByText('Work'))
    expect(openProfileFromNotch).toHaveBeenCalledWith('work')
  })
})
