import type { SegmentedOption, ThemeMode } from '@/design'

import { Monitor, Moon, Sun } from 'lucide-react'

import { Segmented, useTheme } from '@/design'
import { useAppState } from '@/lib/app-state/use-app-state'

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
 * Top section of Settings — theme picker.
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
    </section>
  )
}
