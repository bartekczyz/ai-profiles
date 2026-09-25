import type { ReactNode } from 'react'
import type { AppId, Dependencies, Surfaces } from '@/lib/types'
import type { SurfaceToggle } from '../lib/profile-form'

import { Check, Info } from 'lucide-react'

import { Button, cn } from '@/design'

import { availableSurfaces, dockIconDescription, surfaceToggle } from '../lib/profile-form'

type ProfileSurfaceFieldsProps = {
  /**
   * The app the profile is for, or `''` while none has been chosen.
   */
  app: AppId | ''
  /**
   * The surfaces as chosen, before dropping the ones that are not installed.
   */
  surfaces: Surfaces
  /**
   * Whether the profile gets a Dock icon of its own, as it will be saved.
   */
  distinctDockIcon: boolean
  /**
   * What is installed on this Mac.
   */
  dependencies: Dependencies
  /**
   * Takes the surfaces with one of them switched.
   */
  onSurfacesChange: (next: Surfaces) => void
  /**
   * Takes the Dock icon option's new value.
   */
  onDistinctDockIconChange: (next: boolean) => void
  /**
   * Opens the explanation of the Dock icon option, for reading.
   */
  onExplainDockIcon: () => void
}

/**
 * The two surface toggle cards, with the Dock icon option under the desktop
 * one. A card self-disables when its surface is not installed, and says where
 * to get it underneath.
 */
export function ProfileSurfaceFields({
  app,
  surfaces,
  distinctDockIcon,
  dependencies,
  onSurfacesChange,
  onDistinctDockIconChange,
  onExplainDockIcon,
}: ProfileSurfaceFieldsProps) {
  const available = availableSurfaces(dependencies, app)
  const gui = surfaceToggle('gui', app, available.gui, surfaces.gui)
  const cli = surfaceToggle('cli', app, available.cli, surfaces.cli)
  // The Dock icon belongs to the desktop launcher, so it means nothing without one.
  const desktopLauncher = gui.checked

  return (
    <div className="flex flex-col gap-2.5">
      {/* The Dock icon is an option of the desktop launcher, so it shares its card. */}
      <SurfaceOption toggle={gui} onChange={(next) => onSurfacesChange({ ...surfaces, gui: next })}>
        <ToggleRow
          nested
          checked={distinctDockIcon && desktopLauncher}
          disabled={!desktopLauncher}
          title="Distinct Dock icon"
          description={dockIconDescription(app)}
          info={{ label: 'About the Dock icon', onClick: onExplainDockIcon }}
          onChange={onDistinctDockIconChange}
        />
      </SurfaceOption>
      <SurfaceOption toggle={cli} onChange={(next) => onSurfacesChange({ ...surfaces, cli: next })} />
    </div>
  )
}

type SurfaceOptionProps = {
  /**
   * How the surface's toggle reads.
   */
  toggle: SurfaceToggle
  /**
   * Options of the surface, drawn as further rows of its card.
   */
  children?: ReactNode
  /**
   * Takes the surface's new value.
   */
  onChange: (next: boolean) => void
}

/**
 * A surface's card, followed by where to install the surface when it is missing.
 */
function SurfaceOption({ toggle, children, onChange }: SurfaceOptionProps) {
  return (
    <>
      <SurfaceCard>
        <ToggleRow
          checked={toggle.checked}
          disabled={toggle.disabled}
          title={toggle.title}
          description={toggle.description}
          onChange={onChange}
        />
        {children}
      </SurfaceCard>
      {toggle.install !== null ? (
        <p className="pl-7 font-mono text-mono text-muted-strong">
          Install{' '}
          <a className="underline" href={toggle.install.href} target="_blank" rel="noreferrer">
            {toggle.install.label}
          </a>{' '}
          first.
        </p>
      ) : null}
    </>
  )
}

type SurfaceCardProps = {
  /**
   * The card's rows.
   */
  children: ReactNode
}

/**
 * A surface, drawn as a card of one or more rows, each a toggle of its own.
 */
function SurfaceCard({ children }: SurfaceCardProps) {
  return (
    <div className="divide-y divide-border-soft overflow-hidden rounded-lg border border-border bg-white dark:bg-cream-2">
      {children}
    </div>
  )
}

type ToggleRowProps = {
  checked: boolean
  disabled: boolean
  title: string
  description: string
  /**
   * Set for an option of the row above it, so that it sits under that row's
   * title rather than under its checkbox.
   */
  nested?: boolean
  /**
   * A button on the row that opens an explanation of the option, for an option
   * that needs one. It is a control of its own: pressing it does not toggle the
   * option, and it works while the option is disabled.
   */
  info?: { label: string; onClick: () => void }
  onChange: (next: boolean) => void
}

function ToggleRow({ checked, disabled, title, description, nested = false, info, onChange }: ToggleRowProps) {
  return (
    <div className="relative">
      {/* biome-ignore lint/a11y/useSemanticElements: rich row layout with description copy precludes a native <input type="checkbox"> */}
      <button
        type="button"
        role="checkbox"
        aria-checked={checked}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={cn(
          'flex w-full items-start gap-3 p-3 text-left cursor-pointer transition-colors duration-(--duration-snap) ease-(--ease-natural)',
          'hover:not-disabled:bg-black/[0.02] dark:hover:not-disabled:bg-white/[0.03]',
          // Inside the row, so the card's edge does not clip it.
          'focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-orange/40',
          'disabled:cursor-not-allowed disabled:opacity-60',
          // A nested option lines up with the title of the row above: past its
          // checkbox (16px) and the gap (12px), from the row's own padding (12px).
          nested && 'pl-10',
          // Room for the info button, which sits on the row rather than in it.
          info !== undefined && 'pr-11',
        )}
      >
        <span
          aria-hidden
          className={cn(
            'mt-px grid h-4 w-4 shrink-0 place-items-center rounded-[5px] border-[1.5px] transition-colors duration-(--duration-snap) ease-(--ease-natural)',
            checked ? 'border-orange bg-orange' : 'border-border bg-cream',
          )}
        >
          {checked ? <Check className="h-[11px] w-[11px] text-white" strokeWidth={3} /> : null}
        </span>
        <span className="flex-1">
          <span className="block text-[13px] font-medium text-ink">{title}</span>
          <span className="mt-0.5 block text-[12px] text-muted leading-[1.4]">{description}</span>
        </span>
      </button>
      {info !== undefined ? (
        <Button
          variant="ghost"
          size="sm"
          aria-label={info.label}
          title={info.label}
          leadingIcon={<Info className="h-4 w-4" />}
          className="absolute top-2 right-2 h-6 w-6 px-0"
          onClick={info.onClick}
        />
      ) : null}
    </div>
  )
}
