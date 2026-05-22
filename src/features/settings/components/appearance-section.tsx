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
 *
 * The helper line reads `useTheme().resolved` so it updates in real time
 * when System is selected and the OS appearance flips (the provider
 * subscribes to prefers-color-scheme via useSyncExternalStore).
 */
export function AppearanceSection() {
  const theme = useTheme()
  const appState = useAppState()

  function handleChange(next: ThemeMode) {
    theme.setMode(next)
    void appState.update({ themeMode: next })
  }

  const helperSuffix = theme.mode === 'system' ? 'matches system' : 'explicit'

  return (
    <section className="mb-8">
      <div className="mb-2.5 flex items-center justify-between">
        <span className="font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-muted-strong">
          Appearance
        </span>
      </div>
      <p className="text-[12.5px] leading-[1.55] tracking-[-0.003em] text-muted">
        Choose how claude-profiles renders. System follows your macOS appearance setting.
      </p>
      <div className="mt-3 flex items-center gap-4">
        <Segmented<ThemeMode> ariaLabel="Theme" options={themeOptions} value={theme.mode} onChange={handleChange} />
        <span className="font-mono text-[11px] tracking-normal text-muted-strong" data-testid="theme-helper">
          Currently: {theme.resolved} ({helperSuffix})
        </span>
      </div>
    </section>
  )
}
