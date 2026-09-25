import type { AppId } from '@/lib/app-registry'
import type { ExistingInstallInfo, ExistingInstallSizes, ImportExistingInput, Profile } from '@/lib/types'

import { useState } from 'react'

import { Loader2 } from 'lucide-react'

import { Button, Dialog, Kbd, StatusDot } from '@/design'
import { Input } from '@/design/ui/input'
import { useMigrationSizes } from '@/features/migration/api/use-migration'
// cross-feature: migration dialog reuses the profile color picker for the imported profile
import { ColorSwatchPicker } from '@/features/profiles/components/color-swatch-picker'
import { slugifyPreview } from '@/features/profiles/components/profile-form-fields'
import { isProfileFormValid } from '@/features/profiles/lib/profile-form'
import { appSpecs } from '@/lib/app-registry'
import { presetColors } from '@/lib/colors'
import { formatBytes } from '@/lib/format-bytes'

type Props = {
  open: boolean
  app: AppId
  existing: ExistingInstallInfo
  onClose: () => void
  onImport: (input: ImportExistingInput) => Promise<Profile>
}

function shorten(absolutePath: string): string {
  const home = absolutePath.match(/^\/Users\/[^/]+/)?.[0]
  if (home && absolutePath.startsWith(home)) {
    return `~${absolutePath.slice(home.length)}`
  }
  return absolutePath
}

export function MigrationDialog({ open, app, existing, onClose, onImport }: Props) {
  // Size walks happen off the boot critical path — this hook only fires
  // while the dialog is open, so the size column populates a beat after
  // the dialog appears rather than blocking app startup.
  const sizes = useMigrationSizes(open, app)
  const spec = appSpecs[app]
  const [name, setName] = useState('Default')
  const [color, setColor] = useState<string>(presetColors[0])
  const [includeGui, setIncludeGui] = useState(existing.guiPath !== null)
  const [includeCli, setIncludeCli] = useState(existing.cliPath !== null)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const valid = isProfileFormValid(name, color, { gui: includeGui, cli: includeCli })

  async function handleSubmit() {
    setSubmitting(true)
    setError(null)
    try {
      await onImport({ name: name.trim(), color, includeGui, includeCli })
      onClose()
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog
      open={open}
      title={`Import existing ${spec.displayName} install`}
      description={`Adopt your existing ${spec.displayName} data as your first profile. The originals move to a 7-day backup; the data stays reachable via a new ${spec.cliWrapperPrefix}-<name> command.`}
      onClose={onClose}
      className="w-[min(760px,calc(100%-64px))]"
      foot={<MigrationDialogFoot valid={valid} submitting={submitting} onCancel={onClose} onImport={handleSubmit} />}
    >
      <div className="grid grid-cols-1 gap-6 md:grid-cols-[1fr_minmax(0,300px)]">
        <div className="space-y-5">
          <DetectedSection app={app} existing={existing} sizes={sizes} />

          <ProfileNameField name={name} cliWrapperPrefix={spec.cliWrapperPrefix} onChange={setName} />

          <div>
            <div className="mb-1.5 font-mono text-[11.5px] font-medium uppercase tracking-[0.08em] text-muted">
              Color
            </div>
            <ColorSwatchPicker value={color} onChange={setColor} />
          </div>

          <ImportChoices
            app={app}
            existing={existing}
            includeGui={includeGui}
            includeCli={includeCli}
            onIncludeGuiChange={setIncludeGui}
            onIncludeCliChange={setIncludeCli}
          />

          {error ? <p className="text-meta text-red">{error}</p> : null}
        </div>

        <section>
          <h3 className="mb-2 m-0 font-mono text-eyebrow font-medium uppercase tracking-[0.1em] text-muted-strong">
            What will happen
          </h3>
          <WhatWillHappenCard app={app} />
        </section>
      </div>
    </Dialog>
  )
}

type MigrationDialogFootProps = {
  /**
   * Whether the form holds a profile that can be imported.
   */
  valid: boolean
  /**
   * Whether an import is under way.
   */
  submitting: boolean
  /**
   * Dismisses the dialog.
   */
  onCancel: () => void
  /**
   * Starts the import.
   */
  onImport: () => void
}

/**
 * The dialog's Cancel and Import buttons. Both lock while an import is under
 * way, and Import swaps its shortcut for a spinner.
 */
function MigrationDialogFoot({ valid, submitting, onCancel, onImport }: MigrationDialogFootProps) {
  return (
    <>
      <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={submitting} onClick={onCancel}>
        Cancel
      </Button>
      <Button
        disabled={!valid || submitting}
        variant="primary"
        size="sm"
        leadingIcon={submitting ? <Loader2 aria-hidden className="h-3.5 w-3.5 animate-spin" /> : null}
        trailingKbd={submitting ? null : <Kbd variant="onOrange">⏎</Kbd>}
        onClick={onImport}
      >
        {submitting ? 'Importing…' : 'Import'}
      </Button>
    </>
  )
}

type DetectedSectionProps = {
  /**
   * The app whose install was detected.
   */
  app: AppId
  /**
   * Where the install's desktop and CLI data were found.
   */
  existing: ExistingInstallInfo
  /**
   * How much each detected install holds on disk, once measured.
   */
  sizes: ExistingInstallSizes
}

/**
 * Lists the detected desktop and CLI installs, each with its size and path.
 */
function DetectedSection({ app, existing, sizes }: DetectedSectionProps) {
  const spec = appSpecs[app]
  return (
    <section>
      <div className="mb-2 font-mono text-eyebrow font-medium uppercase tracking-[0.1em] text-muted-strong">
        Detected
      </div>
      <ul className="overflow-hidden rounded-lg border border-border bg-white dark:bg-cream-2">
        {existing.guiPath ? (
          <DetectedRow label={`${spec.displayName} Desktop`} path={existing.guiPath} sizeBytes={sizes.guiSizeBytes} />
        ) : null}
        {existing.cliPath ? (
          <DetectedRow label={`${spec.cliDisplayName} CLI`} path={existing.cliPath} sizeBytes={sizes.cliSizeBytes} />
        ) : null}
      </ul>
    </section>
  )
}

type ProfileNameFieldProps = {
  /**
   * The name as typed.
   */
  name: string
  /**
   * The app's CLI wrapper prefix, which the command preview starts with.
   */
  cliWrapperPrefix: string
  /**
   * Called with the name as the user types it.
   */
  onChange: (name: string) => void
}

/**
 * The profile name input, with a preview of the command the profile will be
 * invoked as.
 */
function ProfileNameField({ name, cliWrapperPrefix, onChange }: ProfileNameFieldProps) {
  const slugPreview = slugifyPreview(name)
  return (
    <div>
      <label
        htmlFor="migration-name"
        className="mb-1.5 block font-mono text-[11.5px] font-medium uppercase tracking-[0.08em] text-muted"
      >
        Profile name
      </label>
      <Input
        autoFocus
        id="migration-name"
        type="text"
        value={name}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        onChange={(event) => onChange(event.target.value)}
      />
      {/* Always rendered so the line reserves its height — no shift when typing. */}
      <p className="mt-1.5 font-mono text-mono text-muted-strong">
        {slugPreview ? (
          <>
            Invoked as <code className="text-ink">{`${cliWrapperPrefix}-${slugPreview}`}</code>
          </>
        ) : (
          '\u00A0'
        )}
      </p>
    </div>
  )
}

type ImportChoicesProps = {
  /**
   * The app whose install is being imported.
   */
  app: AppId
  /**
   * Where the install's desktop and CLI data were found.
   */
  existing: ExistingInstallInfo
  /**
   * Whether the desktop app's data is imported.
   */
  includeGui: boolean
  /**
   * Whether the CLI's config is imported.
   */
  includeCli: boolean
  /**
   * Called when the user ticks or unticks the desktop app's data.
   */
  onIncludeGuiChange: (include: boolean) => void
  /**
   * Called when the user ticks or unticks the CLI's config.
   */
  onIncludeCliChange: (include: boolean) => void
}

/**
 * One checkbox per detected install, choosing what the import takes.
 */
function ImportChoices({
  app,
  existing,
  includeGui,
  includeCli,
  onIncludeGuiChange,
  onIncludeCliChange,
}: ImportChoicesProps) {
  return (
    <fieldset>
      <legend className="mb-1.5 font-mono text-[11.5px] font-medium uppercase tracking-[0.08em] text-muted">
        What to import
      </legend>
      <div className="flex flex-col gap-1.5 text-body text-ink-soft">
        {existing.guiPath ? (
          <label className="flex cursor-pointer items-center gap-2">
            <input
              type="checkbox"
              checked={includeGui}
              onChange={(event) => onIncludeGuiChange(event.target.checked)}
              className="h-3.5 w-3.5 cursor-pointer accent-orange"
            />
            Desktop app data (history, login)
          </label>
        ) : null}
        {existing.cliPath ? (
          <label className="flex cursor-pointer items-center gap-2">
            <input
              type="checkbox"
              checked={includeCli}
              onChange={(event) => onIncludeCliChange(event.target.checked)}
              className="h-3.5 w-3.5 cursor-pointer accent-orange"
            />
            {appSpecs[app].cliDisplayName} CLI config
          </label>
        ) : null}
      </div>
    </fieldset>
  )
}

type DetectedRowProps = {
  label: string
  path: string
  sizeBytes: number | null
}

function DetectedRow({ label, path, sizeBytes }: DetectedRowProps) {
  return (
    <li className="flex items-start gap-3 border-b border-border-soft px-3.5 py-2.5 last:border-b-0">
      <StatusDot pulse tone="success" className="mt-[7px]" />
      <div className="min-w-0 flex-1">
        <div className="text-body text-ink">{label}</div>
        <div className="font-mono text-mono text-muted-strong truncate">
          {sizeBytes !== null ? <span className="text-ink-soft">{formatBytes(sizeBytes)}</span> : null}
          {sizeBytes !== null ? <span className="mx-1.5 text-border">·</span> : null}
          <span>{shorten(path)}</span>
        </div>
      </div>
    </li>
  )
}

function WhatWillHappenCard({ app }: { app: AppId }) {
  const spec = appSpecs[app]
  return (
    <aside className="rounded-lg border border-border-soft bg-white p-4 dark:bg-cream-2">
      <ol className="m-0 list-decimal space-y-2.5 pl-5 text-meta text-ink-soft">
        <li>
          Your existing {spec.displayName} data is <strong className="text-ink">copied</strong> into the new profile
          dir.
        </li>
        <li>
          The originals (<code className="font-mono text-mono">{spec.cliStockConfigDir}</code>,{' '}
          <code className="font-mono text-mono">{spec.guiStockSupportDir}</code>) are{' '}
          <strong className="text-ink">moved to a 7-day backup</strong>. Delete it from Settings any time.
        </li>
        <li>
          From now on, use <code className="font-mono text-mono text-ink">{spec.cliWrapperPrefix}-&lt;name&gt;</code>{' '}
          instead of <code className="font-mono text-mono">{spec.cliBinary}</code>. Plain{' '}
          <code className="font-mono text-mono">{spec.cliBinary}</code> will start a fresh install dir.
        </li>
        <li>
          <strong className="text-ink">{spec.cliDisplayName} CLI:</strong> you'll need to log in once.{' '}
          {app === 'codex' ? (
            <>
              Credentials live in <code className="font-mono text-mono">auth.json</code> under{' '}
              <code className="font-mono text-mono">{spec.cliConfigEnv}</code> — isolated per profile, no Keychain.
            </>
          ) : (
            <>
              macOS Keychain keys are derived from <code className="font-mono text-mono">{spec.cliConfigEnv}</code>.
            </>
          )}
        </li>
      </ol>
    </aside>
  )
}
