import type { SegmentedOption, ThemeMode } from '@/design'

import { Monitor, Moon, Sun } from 'lucide-react'

import { Segmented, useTheme } from '@/design'
import { UsageAlertThresholds } from '@/features/notch/components/usage-alert-thresholds'
import { useAppState } from '@/lib/app-state/use-app-state'

type NotchHud = 'shown' | 'hidden'

const notchOptions: ReadonlyArray<SegmentedOption<NotchHud>> = [
  { value: 'shown', label: 'Shown', ariaLabel: 'Show weekly limits in the notch' },
  { value: 'hidden', label: 'Hidden', ariaLabel: 'Hide weekly limits from the notch' },
]

const themeOptions: ReadonlyArray<SegmentedOption<ThemeMode>> = [
  {
    value: 'light',
    label: 'Light',
    ariaLabel: 'Light theme',
    icon: <Sun className="h-[13px] w-[13px]" strokeWidth={1.75} />,
  },
  {
    value: 'system',
    label: 'System',
    ariaLabel: 'System theme',
    icon: <Monitor className="h-[13px] w-[13px]" strokeWidth={1.75} />,
  },
  {
    value: 'dark',
    label: 'Dark',
    ariaLabel: 'Dark theme',
    icon: <Moon className="h-[13px] w-[13px]" strokeWidth={1.75} />,
  },
]

/**
 * Top section of Settings — theme picker and the notch HUD switch.
 *
 * The segmented control's value mirrors `useTheme().mode` (the live in-memory
 * mode, which the ThemeProvider applies to `<html data-theme>`). Changes flow
 * through two writes:
 * 1. `theme.setMode(next)` — flips the provider immediately so the whole app
 *    repaints with no waiting on the IPC round-trip.
 * 2. `appState.update({ themeMode: next })` — persists the choice to
 *    state.json via the existing optimistic mutation. The corresponding
 *    effect in `app.tsx` becomes a no-op since the two values are already
 *    in sync.
 */
export function AppearanceSection() {
  const theme = useTheme()
  const appState = useAppState()

  function handleChange(next: ThemeMode) {
    theme.setMode(next)
    void appState.update({ themeMode: next })
  }

  return (
    <section className="mb-8">
      <div className="mb-2.5 flex items-center justify-between">
        <span className="font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-muted-strong">
          Appearance
        </span>
      </div>
      <Segmented<ThemeMode> ariaLabel="Theme" options={themeOptions} value={theme.mode} onChange={handleChange} />
      <div className="mt-5 mb-2.5 flex items-center justify-between">
        <span className="font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-muted-strong">
          Weekly limits in the notch
        </span>
      </div>
      <Segmented<NotchHud>
        ariaLabel="Weekly limits in the notch"
        options={notchOptions}
        value={appState.state.notchHudOff ? 'hidden' : 'shown'}
        onChange={(next) => {
          void appState.update({ notchHudOff: next === 'hidden' })
        }}
      />
      <p className="mt-2 text-[12px] text-muted-strong">
        On a Mac with a notch, rest the pointer on it to see every profile's weekly limit.
      </p>
      {appState.state.notchHudOff ? null : (
        <>
          <div className="mt-5 mb-2.5 flex items-center justify-between">
            <span className="font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-muted-strong">
              Usage alerts
            </span>
          </div>
          <UsageAlertThresholds
            value={appState.state.usageAlertThresholds ?? null}
            onChange={(thresholds) => {
              void appState.update({ usageAlertThresholds: thresholds })
            }}
          />
          <p className="mt-2 text-[12px] text-muted-strong">
            The notch drops open when a profile's weekly or 5-hour limit passes one of these. Pick none to turn alerts
            off. Checked every 5 minutes while you're using the Mac.
          </p>
        </>
      )}
    </section>
  )
}
