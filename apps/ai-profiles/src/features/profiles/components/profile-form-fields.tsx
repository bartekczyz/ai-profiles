import type { ReactNode } from 'react'
import type { AppId, Dependencies, RemoteHost, Surfaces } from '@/lib/types'

import { Check, Info } from 'lucide-react'

// cross-feature: form fields use the profile color picker primitive
import { Button, cn } from '@/design'
import { Input } from '@/design/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/design/ui/select'
import { type AppSpec, appIds, appSpecs } from '@/lib/app-registry'
import { presetColors } from '@/lib/colors'

import { ColorSwatchPicker } from './color-swatch-picker'

/** The type that makes a profile on a paired server rather than on this Mac. */
export const remoteType = 'remote'

/** An app on this Mac, a profile on a server, or not chosen yet. */
export type ProfileType = AppId | typeof remoteType | ''

/**
 * A name the server takes for a new account folder: letters, digits, `-` and
 * `_`, starting with a letter or digit, at most 64, and not `default`. Mirrors
 * `valid_new_name` in the server.
 */
export function isValidRemoteProfileName(name: string): boolean {
  return /^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/.test(name) && name !== 'default'
}

type Props = {
  app?: ProfileType
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
  onAppChange?: (app: Exclude<ProfileType, ''>) => void
  /** Offers the remote type: the paired servers, and which is chosen. */
  remote?: {
    hosts: Array<RemoteHost>
    hostId: string
    onHostChange: (hostId: string) => void
    /** Another profile on that server has the name (ignoring case). */
    taken?: boolean
  }
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
  remote,
}: Props) {
  if (app === remoteType && remote) {
    return (
      <RemoteFields
        name={name}
        color={color}
        remote={remote}
        typeField={
          onAppChange !== undefined && installedApps !== undefined ? (
            <TypeField app={app} installedApps={installedApps} remote={remote} onAppChange={onAppChange} />
          ) : null
        }
        onNameChange={onNameChange}
        onColorChange={onColorChange}
      />
    )
  }
  const slugPreview = name.trim().length > 0 ? slugifyPreview(name) : ''
  const resolvedApp = app !== '' && app !== undefined && app !== remoteType ? app : undefined
  const spec = resolvedApp !== undefined ? appSpecs[resolvedApp] : null
  const appDeps = resolvedApp !== undefined ? dependencies.apps[resolvedApp] : null
  // The Dock icon belongs to the desktop launcher, so it means nothing without one.
  const desktopLauncher = surfaces.gui && (appDeps?.guiInstalled ?? false)

  return (
    <div className="space-y-4">
      {onAppChange !== undefined && installedApps !== undefined ? (
        <TypeField app={app} installedApps={installedApps} remote={remote} onAppChange={onAppChange} />
      ) : null}
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
        {/* Always rendered (non-breaking space when empty) so the slug line
            reserves its height and the dialog doesn't shift when typing. */}
        {showSlugPreview ? (
          <p className="mt-1.5 font-mono text-mono text-muted-strong">
            {slugPreview ? `Slug: ${slugPreview}` : '\u00A0'}
          </p>
        ) : null}
      </Field>
      <Field label="Color">
        <ColorSwatchPicker value={color} onChange={onColorChange} />
      </Field>
      <Field label="Surfaces">
        <div className="flex flex-col gap-2.5">
          {/* The Dock icon is an option of the desktop launcher, so it shares its card. */}
          <SurfaceCard>
            <ToggleRow
              checked={surfaces.gui && (appDeps?.guiInstalled ?? false)}
              disabled={!(appDeps?.guiInstalled ?? false)}
              title={spec?.gui.label ?? 'Desktop App launcher'}
              description={spec?.gui.description ?? ''}
              onChange={(next) => onSurfacesChange({ ...surfaces, gui: next })}
            />
            <ToggleRow
              nested
              checked={distinctDockIcon && desktopLauncher}
              disabled={!desktopLauncher}
              title="Distinct Dock icon"
              description={dockIconDescription(spec)}
              info={{ label: 'About the Dock icon', onClick: onExplainDockIcon }}
              onChange={onDistinctDockIconChange}
            />
          </SurfaceCard>
          {appDeps !== null && !appDeps.guiInstalled ? (
            <p className="pl-7 font-mono text-mono text-muted-strong">
              Install{' '}
              <a className="underline" href={spec?.gui.installUrl} target="_blank" rel="noreferrer">
                {spec?.displayName} Desktop
              </a>{' '}
              first.
            </p>
          ) : null}
          <SurfaceCard>
            <ToggleRow
              checked={surfaces.cli && (appDeps?.cliInstalled ?? false)}
              disabled={!(appDeps?.cliInstalled ?? false)}
              title={spec?.cli.label ?? 'CLI wrapper'}
              description={spec?.cli.description ?? ''}
              onChange={(next) => onSurfacesChange({ ...surfaces, cli: next })}
            />
          </SurfaceCard>
          {appDeps !== null && !appDeps.cliInstalled ? (
            <p className="pl-7 font-mono text-mono text-muted-strong">
              Install{' '}
              <a className="underline" href={spec?.cli.installUrl} target="_blank" rel="noreferrer">
                {spec?.cliDisplayName}
              </a>{' '}
              first.
            </p>
          ) : null}
        </div>
      </Field>
    </div>
  )
}

/**
 * What the Dock icon option says next to its checkbox, so the price of it (for
 * ChatGPT, its notifications) is in front of the user before they turn it on,
 * not only in the explanation that follows.
 */
function dockIconDescription(spec: AppSpec | null): string {
  const intro = 'Gives this profile its own Dock icon and name.'
  if (spec === null) {
    return intro
  }
  const cost = spec.dockIcon.cost === null ? '' : ` ${spec.dockIcon.cost}`
  return `${intro}${cost} The default ${spec.displayName} keeps the stock app.`
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

/**
 * A surface, drawn as a card of one or more rows, each a toggle of its own.
 */
function SurfaceCard({ children }: { children: ReactNode }) {
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

export { presetColors }

function TypeField({
  app,
  installedApps,
  remote,
  onAppChange,
}: {
  app: ProfileType | undefined
  installedApps: ReadonlyArray<AppId>
  remote: Props['remote']
  onAppChange: (app: Exclude<ProfileType, ''>) => void
}) {
  return (
    <Field label="Type">
      <Select value={app ?? ''} onValueChange={(value) => onAppChange(value as Exclude<ProfileType, ''>)}>
        <SelectTrigger aria-label="App type" className="w-full">
          <SelectValue placeholder="Choose an app" />
        </SelectTrigger>
        <SelectContent>
          {appIds.map((id) => (
            <SelectItem key={id} disabled={!installedApps.includes(id)} value={id}>
              {appSpecs[id].displayName}
            </SelectItem>
          ))}
          {remote ? (
            <SelectItem disabled={remote.hosts.length === 0} value={remoteType}>
              Claude CLI Remote
            </SelectItem>
          ) : null}
        </SelectContent>
      </Select>
    </Field>
  )
}

/**
 * A profile on a paired server: which server, then a name the server can use
 * as its account folder, and the color it shows in with here. Claude runs on
 * the server, in tmux, so there are no surfaces on this Mac to pick.
 */
function RemoteFields({
  name,
  color,
  remote,
  typeField,
  onNameChange,
  onColorChange,
}: {
  name: string
  color: string
  remote: NonNullable<Props['remote']>
  typeField: ReactNode
  onNameChange: (name: string) => void
  onColorChange: (color: string) => void
}) {
  const host = remote.hosts.find((candidate) => candidate.id === remote.hostId)
  const trimmed = name.trim()
  return (
    <div className="space-y-4">
      {typeField}
      <Field label="Server">
        <Select value={remote.hostId} onValueChange={remote.onHostChange}>
          <SelectTrigger aria-label="Server" className="w-full">
            <SelectValue placeholder="Choose a server" />
          </SelectTrigger>
          <SelectContent>
            {remote.hosts.map((candidate) => (
              <SelectItem key={candidate.id} value={candidate.id}>
                {candidate.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      <Field htmlFor="profile-name" label="Name">
        <Input
          autoFocus
          id="profile-name"
          type="text"
          value={name}
          onChange={(event) => onNameChange(event.target.value)}
          placeholder="work"
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck={false}
        />
        <p
          className={cn(
            'mt-1.5 font-mono text-mono',
            trimmed && (!isValidRemoteProfileName(trimmed) || remote.taken) ? 'text-red' : 'text-muted-strong',
          )}
        >
          {trimmed === ''
            ? '\u00A0'
            : !isValidRemoteProfileName(trimmed)
              ? 'Letters, digits, - and _, starting with a letter or digit. "default" is taken.'
              : remote.taken
                ? `${host?.label ?? 'The server'} already has a profile called ${trimmed}.`
                : `On ${host?.label ?? 'the server'}: ~/.claude-accounts/${trimmed}`}
        </p>
      </Field>
      <Field label="Color">
        <ColorSwatchPicker value={color} onChange={onColorChange} />
      </Field>
      <p className="text-meta text-muted">
        After it's made, you sign it in: its sign-in page opens in your browser, and you paste the code back here.
      </p>
    </div>
  )
}
