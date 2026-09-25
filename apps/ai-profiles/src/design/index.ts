/**
 * Public surface of the design module.
 *
 * The module is closed — nothing here imports from @/components, @/hooks,
 * @/lib, or @tauri-apps/* (enforced by the `design` zone in .fallowrc.json and
 * by boundary.test.ts). Future monorepo split is a `mv src/design
 * packages/design-system/src` plus a tsconfig path edit.
 */

export type { ShortcutId } from './keyboard/shortcuts'
export type { SegmentedOption } from './primitives/segmented'
export type { StatusTone } from './primitives/status-dot'
export type { ThemeMode } from './theme/theme-provider'

export { ariaKeyshortcutsFor } from './keyboard/aria-keyshortcuts'
export { useShortcut } from './keyboard/use-shortcut'
export { cn } from './lib/cn'
export { Button } from './primitives/button'
export { Card } from './primitives/card'
export { Dialog } from './primitives/dialog'
export { Kbd } from './primitives/kbd'
export { Segmented } from './primitives/segmented'
export { Skeleton } from './primitives/skeleton'
export { StatusDot } from './primitives/status-dot'
export { ToastProvider, useToast } from './primitives/toast'
export { TooltipBubble } from './primitives/tooltip-bubble'
export { ThemeProvider, useTheme } from './theme/theme-provider'
