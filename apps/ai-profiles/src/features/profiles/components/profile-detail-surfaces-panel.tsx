import type { ReactNode } from 'react'

import { useEffect, useRef, useState } from 'react'

import { ArrowUpRight, Check, Copy, Monitor, TerminalWindow } from '@phosphor-icons/react'

import { ariaKeyshortcutsFor, cn, Skeleton, useShortcut } from '@/design'

type Props = {
  /**
   * The exact shell command the terminal row copies — the `claude-<slug>`
   * wrapper for a managed profile, the bare binary for a stock install.
   */
  command: string
  guiEnabled: boolean
  cliEnabled: boolean
  /**
   * Whether this pane's ⏎ and ⌘C bindings are live. The panel cannot see
   * the dialogs, palette, and settings pane that cover it, so the app shell
   * decides; the flag is passed through rather than defaulted because an
   * omitted `enabled` reads as `undefined` and silently kills the binding.
   */
  shortcutsEnabled: boolean
  /**
   * Whether a desktop launch is under way. Owned above the detail pane's
   * `Suspense` boundary, because this panel is swapped for an identical one
   * when the profile's paths land — see `useGuiLaunch`.
   */
  opening: boolean
  /**
   * One short line about the desktop surface's own state, replacing the
   * card explainers. Undefined renders a skeleton bar — the per-profile
   * paths that describe the surface are still resolving.
   */
  guiDescription?: string
  cliDescription?: string
  onLaunchGui: () => Promise<unknown>
  onCopyCli: () => Promise<unknown>
  onError: (message: string | null) => void
}

/** How long the token stays swapped to its confirmation after a copy. */
const copiedResetMs = 1200

const offDescription = 'Off — turn on in Edit'
const bothOffDescription = 'Both surfaces off — turn one on in Edit'

const rowClasses =
  'flex min-h-[42px] items-center justify-between gap-3.5 border-t border-border-soft px-[13px] py-[9px] first:border-t-0'

const controlClasses =
  'inline-flex h-7 shrink-0 cursor-default items-center rounded-md leading-none outline-none transition-[background-color,border-color,color,filter,transform] duration-(--duration-snap) ease-(--ease-natural) focus-visible:ring-2 focus-visible:ring-orange/40'

/**
 * The pane's one primary treatment, in ink rather than a brand colour. Worn
 * by the Open button, and by the command token when the terminal is the only
 * surface a profile has — so every reachable state of the pane keeps exactly
 * one obvious action. No hover change: native buttons don't light up under
 * the pointer, they respond to the press.
 */
const filledClasses = 'border-0 bg-ink text-cream shadow-[0_1px_2px_rgba(0,0,0,0.12)] active:opacity-80'

const outlinedClasses = 'border border-border bg-cream text-ink-soft active:bg-cream-2 dark:bg-white/[0.04]'

/**
 * Worn by the Open button while a launch is under way. The fill stays — this
 * is the pane's one primary action and greying it out would read as switched
 * off — but everything that invites another press is taken away.
 */
const busyClasses =
  'disabled:cursor-default disabled:opacity-80 disabled:hover:brightness-100 disabled:active:translate-y-0'

/**
 * The surfaces block: one inset grouped panel holding a Desktop app row and
 * a TerminalWindow row, in the manner of macOS System Settings. Each row carries a
 * glyph, a title, a line describing that surface's own state, and its own
 * trailing control.
 *
 * Because every row states its own status, the merged status line below the
 * panel would only repeat them — it renders only when both surfaces are off,
 * which is the one state no row can explain on its own.
 *
 * A disabled surface greys its own row's control to an em dash and says so
 * in its own description. The container is never dimmed: one switched-off
 * surface must not degrade the legibility of the other.
 *
 * Launch and copy arrive as callbacks so the same panel serves managed
 * profiles (which stamp last-used) and the stock-install entry (which shells
 * out directly).
 */
export function ProfileDetailSurfacesPanel({
  command,
  guiEnabled,
  cliEnabled,
  shortcutsEnabled,
  opening,
  guiDescription,
  cliDescription,
  onLaunchGui,
  onCopyCli,
  onError,
}: Props) {
  const { copied, flashCopied } = useCopiedFlash()

  async function launch(): Promise<void> {
    await runReportingErrors(onLaunchGui, onError)
  }

  async function copy(): Promise<void> {
    const succeeded = await runReportingErrors(onCopyCli, onError)
    if (!succeeded) {
      return
    }
    flashCopied()
  }

  // Registered here rather than in the app shell so the keyboard route runs
  // the button's handler verbatim — same last-used stamp, same error
  // surfacing, same copy confirmation on the token.
  useShortcut(
    'open-selected-desktop',
    () => {
      void launch()
    },
    { enabled: shortcutsEnabled && guiEnabled },
  )
  useShortcut(
    'copy-selected-cli',
    () => {
      void copy()
    },
    { enabled: shortcutsEnabled && cliEnabled },
  )

  // With no Open button on screen, the token is the only action left, so it
  // takes the primary fill. Checked against the outlined alternative: mono
  // white-on-orange still reads as a shell command, and the alternative
  // leaves a pane with nothing coloured to aim at.
  const tokenPromoted = cliEnabled && !guiEnabled

  return (
    <>
      <div className="h-full overflow-hidden rounded-[10px] border border-border bg-cream/70 dark:bg-white/[0.02]">
        <SurfaceRow
          enabled={guiEnabled}
          glyph={<Monitor aria-hidden className="h-3.5 w-3.5" />}
          title="Desktop app"
          description={guiDescription}
          control={
            <OpenDesktopButton
              opening={opening}
              onOpen={() => {
                void launch()
              }}
            />
          }
        />
        <SurfaceRow
          enabled={cliEnabled}
          glyph={<TerminalWindow aria-hidden className="h-3.5 w-3.5" />}
          title="Terminal"
          description={cliDescription}
          control={
            <CopyCommandButton
              copied={copied}
              promoted={tokenPromoted}
              command={command}
              onCopy={() => {
                void copy()
              }}
            />
          }
        />
      </div>

      {!guiEnabled && !cliEnabled ? (
        <p role="status" className="mt-3.5 flex items-center gap-[7px] text-meta text-muted">
          <span aria-hidden className="h-1.5 w-1.5 shrink-0 rounded-full bg-muted-strong" />
          {bothOffDescription}
        </p>
      ) : null}
    </>
  )
}

/**
 * Runs a surface action and reports how it went: clears the pane's error on
 * success, sets it to the failure's message otherwise. Resolves to whether the
 * action succeeded.
 */
async function runReportingErrors(
  action: () => Promise<unknown>,
  onError: (message: string | null) => void,
): Promise<boolean> {
  try {
    await action()
    onError(null)
    return true
  } catch (caught) {
    onError(caught instanceof Error ? caught.message : String(caught))
    return false
  }
}

/**
 * The command token's copy confirmation: `copied` turns on with `flashCopied`
 * and back off `copiedResetMs` later, restarting the wait on a repeat copy.
 */
function useCopiedFlash() {
  const [copied, setCopied] = useState(false)
  const resetTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    return () => {
      if (resetTimer.current !== null) {
        clearTimeout(resetTimer.current)
      }
    }
  }, [])

  function flashCopied(): void {
    setCopied(true)
    if (resetTimer.current !== null) {
      clearTimeout(resetTimer.current)
    }
    resetTimer.current = setTimeout(() => setCopied(false), copiedResetMs)
  }

  return { copied, flashCopied }
}

type OpenDesktopButtonProps = {
  /**
   * Whether a desktop launch is under way.
   */
  opening: boolean
  /**
   * Launches the desktop app.
   */
  onOpen: () => void
}

/**
 * The Desktop app row's Open button, busy while a launch is under way.
 */
function OpenDesktopButton({ opening, onOpen }: OpenDesktopButtonProps) {
  return (
    <button
      type="button"
      disabled={opening}
      aria-busy={opening}
      aria-keyshortcuts={ariaKeyshortcutsFor('open-selected-desktop')}
      title="Open (↵)"
      className={cn(controlClasses, filledClasses, busyClasses, 'gap-1.5 px-3 text-[12.5px] font-medium')}
      onClick={onOpen}
    >
      {opening ? 'Opening…' : 'Open'}
      <ArrowUpRight aria-hidden className="size-3.5 opacity-60" />
    </button>
  )
}

type CopyCommandButtonProps = {
  /**
   * Whether the token shows its copy confirmation instead of the command.
   */
  copied: boolean
  /**
   * Whether the token takes the primary fill, as the pane's only action.
   */
  promoted: boolean
  /**
   * The shell command the token shows and copies.
   */
  command: string
  /**
   * Copies the command.
   */
  onCopy: () => void
}

/**
 * The TerminalWindow row's command token, which copies the command on press.
 */
function CopyCommandButton({ copied, promoted, command, onCopy }: CopyCommandButtonProps) {
  return (
    <button
      type="button"
      data-copied={copied ? 'true' : 'false'}
      aria-keyshortcuts={ariaKeyshortcutsFor('copy-selected-cli')}
      className={cn(
        controlClasses,
        promoted ? filledClasses : outlinedClasses,
        'gap-2 px-2.5 font-mono text-[12px]',
        !promoted && 'data-[copied=true]:text-green',
      )}
      title="Copy command (⌘C)"
      onClick={onCopy}
    >
      {copied ? 'Copied' : command}
      {copied ? <Check aria-hidden className="size-3.5" /> : <Copy aria-hidden className="size-3.5 opacity-50" />}
    </button>
  )
}

type SurfaceRowProps = {
  /**
   * Whether the surface is on. A switched-off row says so and greys its
   * control to an em dash.
   */
  enabled: boolean
  glyph: ReactNode
  title: string
  control: ReactNode
  /**
   * Undefined while the paths behind the description are still resolving.
   */
  description?: string
}

function SurfaceRow({ enabled, glyph, title, control, description }: SurfaceRowProps) {
  const shownDescription = enabled ? description : offDescription
  return (
    <div className={rowClasses}>
      <span className="flex min-w-0 items-center gap-[9px]">
        <span
          aria-hidden
          className="grid h-6 w-6 shrink-0 place-items-center rounded-[7px] bg-cream-3 text-muted dark:bg-white/[0.06]"
        >
          {glyph}
        </span>
        <span className="min-w-0">
          <span className="block text-[12.5px] tracking-[-0.005em] text-ink">{title}</span>
          {shownDescription === undefined ? (
            <Skeleton shape="text" className="mt-1 h-2.5 w-44" />
          ) : (
            <span className="block text-[11px] text-muted-strong">{shownDescription}</span>
          )}
        </span>
      </span>
      {enabled ? (
        control
      ) : (
        <span aria-hidden className="text-[11px] text-muted-strong">
          —
        </span>
      )}
    </div>
  )
}
