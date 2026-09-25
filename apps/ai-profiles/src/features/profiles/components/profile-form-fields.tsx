import type { ReactNode } from 'react'
import type { AppId, Dependencies, Surfaces } from '@/lib/types'

import { Input } from '@/design/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/design/ui/select'
import { appIds, appSpecs } from '@/lib/app-registry'

import { ColorSwatchPicker } from './color-swatch-picker'
import { ProfileSurfaceFields } from './profile-surface-fields'

type Props = {
  app: AppId | ''
  name: string
  color: string
  surfaces: Surfaces
  /**
   * Whether the profile gets a Dock icon of its own. Shown as it will be saved,
   * so it reads as off whenever the desktop launcher is.
   */
  distinctDockIcon: boolean
  dependencies: Dependencies
  installedApps?: ReadonlyArray<AppId>
  showSlugPreview?: boolean
  onAppChange?: (app: AppId) => void
  onNameChange: (name: string) => void
  onColorChange: (color: string) => void
  onSurfacesChange: (next: Surfaces) => void
  onDistinctDockIconChange: (next: boolean) => void
  /**
   * Opens the explanation of the Dock icon option, for reading.
   */
  onExplainDockIcon: () => void
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
 * Shared form body for the create and edit modals.
 *
 * Layout: app-type Select, tracked-uppercase eyebrow label + name input + live
 * slug helper, color swatch row, two surface toggle cards with the Dock icon
 * option under the desktop one. Surface cards self-disable when the underlying
 * dependency is missing; the parent renders the actionable copy ("Install …
 * first") underneath if it cares.
 */
export function ProfileFormFields({
  app,
  name,
  color,
  surfaces,
  distinctDockIcon,
  dependencies,
  installedApps,
  showSlugPreview = true,
  onAppChange,
  onNameChange,
  onColorChange,
  onSurfacesChange,
  onDistinctDockIconChange,
  onExplainDockIcon,
}: Props) {
  return (
    <div className="space-y-4">
      {onAppChange !== undefined && installedApps !== undefined ? (
        <AppTypeField app={app} installedApps={installedApps} onAppChange={onAppChange} />
      ) : null}
      <NameField name={name} showSlugPreview={showSlugPreview} onNameChange={onNameChange} />
      <Field label="Color">
        <ColorSwatchPicker value={color} onChange={onColorChange} />
      </Field>
      <Field label="Surfaces">
        <ProfileSurfaceFields
          app={app}
          surfaces={surfaces}
          distinctDockIcon={distinctDockIcon}
          dependencies={dependencies}
          onSurfacesChange={onSurfacesChange}
          onDistinctDockIconChange={onDistinctDockIconChange}
          onExplainDockIcon={onExplainDockIcon}
        />
      </Field>
    </div>
  )
}

type AppTypeFieldProps = {
  /**
   * The chosen app, or `''` while none has been chosen.
   */
  app: AppId | ''
  /**
   * The apps that can be chosen; the others are listed, disabled.
   */
  installedApps: ReadonlyArray<AppId>
  /**
   * Takes the newly chosen app.
   */
  onAppChange: (app: AppId) => void
}

/**
 * The app-type Select, for a profile whose app is still to be chosen.
 */
function AppTypeField({ app, installedApps, onAppChange }: AppTypeFieldProps) {
  return (
    <Field label="Type">
      <Select value={app} onValueChange={(value) => onAppChange(value as AppId)}>
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
  )
}

type NameFieldProps = {
  /**
   * The name as typed.
   */
  name: string
  /**
   * Whether to show the slug the name will get under the input.
   */
  showSlugPreview: boolean
  /**
   * Takes the name as typed.
   */
  onNameChange: (name: string) => void
}

/**
 * The name input, with the live slug helper under it.
 */
function NameField({ name, showSlugPreview, onNameChange }: NameFieldProps) {
  return (
    <Field htmlFor="profile-name" label="Name">
      <Input
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
      />
      {showSlugPreview ? <SlugPreview name={name} /> : null}
    </Field>
  )
}

type SlugPreviewProps = {
  /**
   * The name as typed.
   */
  name: string
}

/**
 * The slug the name will get, as the server will derive it.
 */
function SlugPreview({ name }: SlugPreviewProps) {
  const slug = slugifyPreview(name)
  // Always rendered (non-breaking space when empty) so the slug line reserves
  // its height and the dialog doesn't shift when typing.
  return <p className="mt-1.5 font-mono text-mono text-muted-strong">{slug ? `Slug: ${slug}` : '\u00A0'}</p>
}

type FieldProps = {
  label: string
  htmlFor?: string
  children: ReactNode
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
