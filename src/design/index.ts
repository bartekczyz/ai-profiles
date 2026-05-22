/**
 * Public surface of the design module.
 *
 * The module is closed — nothing here imports from @/components, @/hooks,
 * @/lib, or @tauri-apps/*. Future monorepo split is a `mv src/design
 * packages/design-system/src` plus a tsconfig path edit.
 */

export type { ButtonProps, ButtonSize, ButtonVariant } from './primitives/button'
export type { CardProps, CardRowProps } from './primitives/card'
export type { KbdProps, KbdVariant } from './primitives/kbd'
export type { SegmentedOption, SegmentedProps } from './primitives/segmented'
export type { StatusDotProps, StatusTone } from './primitives/status-dot'
export type { PulseTone, StatusPulseProps } from './primitives/status-pulse'
export type { ResolvedTheme, ThemeMode } from './theme/theme-provider'

export { cn } from './lib/cn'
export { Button } from './primitives/button'
export { Card, CardRow } from './primitives/card'
export { Kbd, KbdGroup } from './primitives/kbd'
export { Segmented } from './primitives/segmented'
export { StatusDot } from './primitives/status-dot'
export { StatusPulse } from './primitives/status-pulse'
export { ThemeProvider, useTheme } from './theme/theme-provider'
