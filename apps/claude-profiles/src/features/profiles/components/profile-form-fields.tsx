import type { AppId, Dependencies, Surfaces } from '@/lib/types'

import { Check } from 'lucide-react'

// cross-feature: form fields use the profile color picker primitive
import { cn } from '@/design'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/design/ui/select'
import { appIds, appSpecs } from '@/lib/app-registry'
import { presetColors } from '@/lib/colors'

import { ColorSwatchPicker } from './color-swatch-picker'

type Props = {
  app?: AppId | ''
  name: string
  color: string
  surfaces: Surfaces
  dependencies: Dependencies
  installedApps?: ReadonlyArray<AppId>
  showSlugPreview?: boolean
  onAppChange?: (app: AppId) => void
  onNameChange: (name: string) => void
  onColorChange: (color: string) => void
  onSurfacesChange: (next: Surfaces) => void
}

/**
 * Mirror of `slugify` in src-tauri/src/slug.rs — kept for the live preview
 * only. The persisted slug is whatever the server returns from
 * createProfile / updateProfile.
 */
export function slugifyPreview(name: string): string {
  let result = ''
  let lastWasDash = true
  for (const character of name) {
    if (/[a-zA-Z0-9]/.test(character)) {
      result += character.toLowerCase()
      lastWasDash = false
    } else if (!lastWasDash) {
      result += '-'
      lastWasDash = true
    }
  }
  return result.replace(/-+$/, '')
}

/**
 * Shared form body for the create modal.
 *
 * Layout: app-type Select, tracked-uppercase eyebrow label + name input + live
 * slug helper, color swatch row, two surface toggle cards. Surface cards
 * self-disable when the underlying dependency is missing; the parent renders
 * the actionable copy ("Install … first") underneath if it cares.
 */
export function ProfileFormFields({
  app,
  name,
  color,
  surfaces,
  dependencies,
  installedApps,
  showSlugPreview = true,
  onAppChange,
  onNameChange,
  onColorChange,
  onSurfacesChange,
}: Props) {
  const slugPreview = name.trim().length > 0 ? slugifyPreview(name) : ''
  const resolvedApp = app !== '' && app !== undefined ? app : undefined
  const spec = resolvedApp !== undefined ? appSpecs[resolvedApp] : null
  const appDeps = resolvedApp !== undefined ? dependencies.apps[resolvedApp] : null

  return (
    <div className="space-y-4">
      {onAppChange !== undefined && installedApps !== undefined ? (
        <Field label="Type">
          <Select value={app ?? ''} onValueChange={(value) => onAppChange(value as AppId)}>
            <SelectTrigger aria-label="App type" className="w-full">
              <SelectValue placeholder="Choose an app" />
            </SelectTrigger>
            <SelectContent>
              {appIds.map((id) => (
                <SelectItem key={id} disabled={!installedApps.includes(id)} value={id}>
                  {appSpecs[id].displayName}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
      ) : null}
      <Field htmlFor="profile-name" label="Name">
        <input
          // biome-ignore lint/a11y/noAutofocus: focus inside a modal lands on the primary input by convention
          autoFocus
          id="profile-name"
          type="text"
          value={name}
          onChange={(event) => onNameChange(event.target.value)}
          placeholder="Personal"
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck={false}
          className="w-full appearance-none rounded-md border border-border bg-white px-3 py-2.5 font-sans text-[13.5px] text-ink outline-none transition-[border-color,box-shadow] duration-(--duration-snap) ease-(--ease-natural) focus:border-orange focus:shadow-[0_0_0_3px_color-mix(in_oklab,var(--color-orange)_15%,transparent)] dark:bg-cream-2"
        />
        {showSlugPreview && slugPreview ? (
          <p className="mt-1.5 font-mono text-mono text-muted-strong">Slug: {slugPreview}</p>
        ) : null}
      </Field>
      <Field label="Color">
        <ColorSwatchPicker value={color} onChange={onColorChange} />
      </Field>
      <Field label="Surfaces">
        <div className="flex flex-col gap-2.5">
          <SurfaceToggle
            checked={surfaces.gui && (appDeps?.guiInstalled ?? false)}
            disabled={!(appDeps?.guiInstalled ?? false)}
            title={spec?.gui.label ?? 'Desktop App launcher'}
            description={spec?.gui.description ?? ''}
            onChange={(next) => onSurfacesChange({ ...surfaces, gui: next })}
          />
          {appDeps !== null && !appDeps.guiInstalled ? (
            <p className="pl-7 font-mono text-mono text-muted-strong">
              Install{' '}
              <a className="underline" href={spec?.gui.installUrl} target="_blank" rel="noreferrer">
                {spec?.displayName} Desktop
              </a>{' '}
              first.
            </p>
          ) : null}
          <SurfaceToggle
            checked={surfaces.cli && (appDeps?.cliInstalled ?? false)}
            disabled={!(appDeps?.cliInstalled ?? false)}
            title={spec?.cli.label ?? 'CLI wrapper'}
            description={spec?.cli.description ?? ''}
            onChange={(next) => onSurfacesChange({ ...surfaces, cli: next })}
          />
          {appDeps !== null && !appDeps.cliInstalled ? (
            <p className="pl-7 font-mono text-mono text-muted-strong">
              Install{' '}
              <a className="underline" href={spec?.cli.installUrl} target="_blank" rel="noreferrer">
                {spec?.displayName}
              </a>{' '}
              first.
            </p>
          ) : null}
        </div>
      </Field>
    </div>
  )
}

type FieldProps = {
  label: string
  htmlFor?: string
  children: React.ReactNode
}

function Field({ label, htmlFor, children }: FieldProps) {
  return (
    <div>
      <label
        htmlFor={htmlFor}
        className="mb-1.5 block font-mono text-[11.5px] font-medium uppercase tracking-[0.08em] text-muted"
      >
        {label}
      </label>
      {children}
    </div>
  )
}

type SurfaceToggleProps = {
  checked: boolean
  disabled: boolean
  title: string
  description: string
  onChange: (next: boolean) => void
}

function SurfaceToggle({ checked, disabled, title, description, onChange }: SurfaceToggleProps) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: rich card layout with description copy precludes a native <input type="checkbox">
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        'flex w-full items-start gap-3 rounded-lg border border-border bg-white p-3 text-left cursor-pointer transition-[border-color,background-color] duration-(--duration-snap) ease-(--ease-natural)',
        'hover:not-disabled:border-border-strong',
        'disabled:cursor-not-allowed disabled:opacity-60',
        'dark:bg-cream-2',
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
  )
}

export { presetColors }
