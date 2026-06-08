/**
 * Public surface of the design module.
 *
 * The module is closed — nothing here imports from @/components, @/hooks,
 * @/lib, or @tauri-apps/*. Future monorepo split is a `mv src/design
 * packages/design-system/src` plus a tsconfig path edit.
 */

export type { Scope, ShortcutDefinition, ShortcutId } from './keyboard/shortcuts'
export type { ButtonProps, ButtonSize, ButtonVariant } from './primitives/button'
export type { CardProps, CardRowProps } from './primitives/card'
export type { DialogProps } from './primitives/dialog'
export type { KbdProps, KbdVariant } from './primitives/kbd'
export type { SegmentedOption, SegmentedProps } from './primitives/segmented'
export type { SkeletonProps, SkeletonShape } from './primitives/skeleton'
export type { StatusDotProps, StatusTone } from './primitives/status-dot'
export type { PulseTone, StatusPulseProps } from './primitives/status-pulse'
export type { ToastTone } from './primitives/toast'
export type { TooltipBubbleProps } from './primitives/tooltip-bubble'
export type { ResolvedTheme, ThemeMode } from './theme/theme-provider'

export { ariaKeyshortcuts, ariaKeyshortcutsFor } from './keyboard/aria-keyshortcuts'
export { formatShortcut } from './keyboard/format-shortcut'
export { getShortcut, shortcuts } from './keyboard/shortcuts'
export { useShortcut } from './keyboard/use-shortcut'
export { cn } from './lib/cn'
export { Button } from './primitives/button'
export { Card, CardRow } from './primitives/card'
export { Dialog } from './primitives/dialog'
export { Kbd, KbdGroup } from './primitives/kbd'
export { Segmented } from './primitives/segmented'
export { Skeleton } from './primitives/skeleton'
export { StatusDot } from './primitives/status-dot'
export { StatusPulse } from './primitives/status-pulse'
export { ToastProvider, useToast } from './primitives/toast'
export { TooltipBubble } from './primitives/tooltip-bubble'
export { ThemeProvider, useTheme } from './theme/theme-provider'
