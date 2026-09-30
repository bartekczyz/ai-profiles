import type { ShortcutId } from '@/design'

import { MoreHorizontal } from 'lucide-react'

import { ariaKeyshortcutsFor, cn, Kbd, useShortcut } from '@/design'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/design/ui/dropdown-menu'
import { openInFinder } from '@/lib/commands'

import { useProfilePaths } from '../api/use-profile-paths'
import { headerControlClasses } from './profile-detail-header'
import { shortenHomePath } from './shorten-home-path'

type Props = {
  profileId: string
  onError: (message: string | null) => void
  /**
   * Omitted for a stock install, which is not ours to delete. The Delete row
   * and its separator are then absent rather than disabled.
   */
  onDelete?: () => void
  /**
   * Offered only for a stock install, which has no Edit dialog to rename or recolor it
   * from. Absent on managed profiles.
   */
  onEdit?: () => void
}

type RevealTarget = {
  shortcutId: ShortcutId
  label: string
  path: string | null
}

type OfferedTarget = RevealTarget & { path: string }

const triggerClasses = 'w-7 px-0 text-muted hover:not-disabled:text-ink'

/**
 * Everything rare about a profile, behind one "…" trigger: the four
 * reveal-in-Finder destinations and Delete.
 *
 * Each reveal row shows its real resolved path as secondary text — the path
 * is what stops "CLI config" and "CLI wrapper" being cryptic — and carries
 * the ⌥1–⌥4 chip, which is now the only place those chips appear. The
 * shortcuts themselves live here too, next to the actions they fire.
 *
 * Destinations that don't exist for this entry are simply not offered — a
 * stock install has no generated wrapper, and an uninstalled desktop app has
 * no launcher, so those rows and their shortcuts drop out on their own. That
 * filtering is what lets the stock pane reuse this component wholesale.
 *
 * The reveal shortcuts are gated on the path existing, not on the surface
 * being switched on: the directories are there either way, and gating the
 * key while still offering the menu row would be inconsistent.
 */
export function ProfileDetailOverflowMenu({ profileId, onError, onDelete, onEdit }: Props) {
  const paths = useProfilePaths(profileId)

  async function reveal(path: string | null): Promise<void> {
    if (path === null) {
      return
    }
    try {
      await openInFinder(path)
      onError(null)
    } catch (caught) {
      onError(caught instanceof Error ? caught.message : String(caught))
    }
  }

  // `enabled` is spread over the hotkey library's defaults, so an omitted
  // value reads as `undefined` and silently disables the binding — always
  // pass it explicitly.
  useShortcut('reveal-gui-data', () => reveal(paths.guiDataDir), { enabled: true })
  useShortcut('reveal-gui-launcher', () => reveal(paths.guiLauncherPath), {
    enabled: paths.guiLauncherPath !== null,
  })
  useShortcut('reveal-cli-config', () => reveal(paths.cliConfigDir), { enabled: true })
  useShortcut('reveal-cli-wrapper', () => reveal(paths.cliWrapperPath), {
    enabled: paths.cliWrapperPath !== null,
  })

  const targets: ReadonlyArray<RevealTarget> = [
    { shortcutId: 'reveal-gui-data', label: 'Desktop app data', path: paths.guiDataDir },
    { shortcutId: 'reveal-gui-launcher', label: 'Launcher', path: paths.guiLauncherPath },
    { shortcutId: 'reveal-cli-config', label: 'CLI config', path: paths.cliConfigDir },
    { shortcutId: 'reveal-cli-wrapper', label: 'CLI wrapper', path: paths.cliWrapperPath },
  ]
  const offered = targets.filter((target): target is OfferedTarget => target.path !== null)

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button type="button" aria-label="More actions" className={cn(headerControlClasses, triggerClasses)}>
          <MoreHorizontal aria-hidden className="h-4 w-4" />
        </button>
      </DropdownMenuTrigger>
      {/* The generated content pins its width to the trigger, which here is a
          28px icon button — override it or the resolved paths wrap to shreds. */}
      <DropdownMenuContent align="end" className="w-72">
        <DropdownMenuLabel className="px-2 pt-1.5 pb-1 font-mono text-eyebrow font-medium uppercase tracking-[0.1em] text-muted-strong">
          Reveal in Finder
        </DropdownMenuLabel>
        {offered.map((target) => (
          <DropdownMenuItem
            key={target.shortcutId}
            className="flex-col items-stretch gap-0.5 px-2 py-1.5 text-[12px]"
            aria-keyshortcuts={ariaKeyshortcutsFor(target.shortcutId)}
            onSelect={() => reveal(target.path)}
          >
            <span className="flex items-center gap-2">
              <span className="min-w-0 flex-1 truncate">{target.label}</span>
              <Kbd variant="subtle" shortcutId={target.shortcutId} />
            </span>
            <span className="truncate font-mono text-[10px] text-muted-strong">{shortenHomePath(target.path)}</span>
          </DropdownMenuItem>
        ))}
        {onEdit === undefined ? null : (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem className="px-2 py-1.5 text-[12px]" onSelect={onEdit}>
              <span className="flex-1">Edit name and color…</span>
            </DropdownMenuItem>
          </>
        )}
        {onDelete === undefined ? null : (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              variant="destructive"
              className="px-2 py-1.5 text-[12px]"
              aria-keyshortcuts={ariaKeyshortcutsFor('delete-selected')}
              onSelect={onDelete}
            >
              <span className="flex-1">Delete profile…</span>
              <Kbd variant="subtle" shortcutId="delete-selected" />
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

/**
 * Suspense fallback for the menu — the trigger keeps its footprint while the
 * paths resolve, so the header doesn't jump on a profile switch. Hidden from
 * assistive technology because there is nothing to open yet.
 */
export function ProfileDetailOverflowMenuFallback() {
  return (
    <button aria-hidden disabled type="button" className={cn(headerControlClasses, triggerClasses)}>
      <MoreHorizontal className="h-4 w-4" />
    </button>
  )
}
