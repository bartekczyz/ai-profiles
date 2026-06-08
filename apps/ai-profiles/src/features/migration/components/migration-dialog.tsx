import type { AppId } from '@/lib/app-registry'
import type { ExistingInstallInfo, ImportExistingInput, Profile } from '@/lib/types'

import { useState } from 'react'

import { Loader2 } from 'lucide-react'

import { Button, Dialog, Kbd, StatusDot } from '@/design'
import { Input } from '@/design/ui/input'
import { useMigrationSizes } from '@/features/migration/api/use-migration'
// cross-feature: migration dialog reuses the profile color picker for the imported profile
import { ColorSwatchPicker } from '@/features/profiles/components/color-swatch-picker'
import { slugifyPreview } from '@/features/profiles/components/profile-form-fields'
import { appSpecs } from '@/lib/app-registry'
import { isValidHexColor, presetColors } from '@/lib/colors'
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

  const canSubmit = name.trim().length > 0 && isValidHexColor(color) && (includeGui || includeCli) && !submitting
  const slugPreview = name.trim().length > 0 ? slugifyPreview(name) : ''

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
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={submitting} onClick={onClose}>
            Cancel
          </Button>
          <Button
            disabled={!canSubmit}
            variant="primary"
            size="sm"
            leadingIcon={submitting ? <Loader2 aria-hidden className="h-3.5 w-3.5 animate-spin" /> : null}
            trailingKbd={submitting ? null : <Kbd variant="onOrange">⏎</Kbd>}
            onClick={handleSubmit}
          >
            {submitting ? 'Importing…' : 'Import'}
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-1 gap-6 md:grid-cols-[1fr_minmax(0,300px)]">
        <div className="space-y-5">
          <section>
            <div className="mb-2 font-mono text-eyebrow font-medium uppercase tracking-[0.1em] text-muted-strong">
              Detected
            </div>
            <ul className="overflow-hidden rounded-lg border border-border bg-white dark:bg-cream-2">
              {existing.guiPath ? (
                <DetectedRow
                  label={`${spec.displayName} Desktop`}
                  path={existing.guiPath}
                  sizeBytes={sizes.guiSizeBytes}
                />
              ) : null}
              {existing.cliPath ? (
                <DetectedRow label={`${spec.displayName} CLI`} path={existing.cliPath} sizeBytes={sizes.cliSizeBytes} />
              ) : null}
            </ul>
          </section>

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
              onChange={(event) => setName(event.target.value)}
            />
            {/* Always rendered so the line reserves its height — no shift when typing. */}
            <p className="mt-1.5 font-mono text-mono text-muted-strong">
              {slugPreview ? (
                <>
                  Invoked as <code className="text-ink">{`${spec.cliWrapperPrefix}-${slugPreview}`}</code>
                </>
              ) : (
                '\u00A0'
              )}
            </p>
          </div>

          <div>
            <div className="mb-1.5 font-mono text-[11.5px] font-medium uppercase tracking-[0.08em] text-muted">
              Color
            </div>
            <ColorSwatchPicker value={color} onChange={setColor} />
          </div>

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
                    onChange={(event) => setIncludeGui(event.target.checked)}
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
                    onChange={(event) => setIncludeCli(event.target.checked)}
                    className="h-3.5 w-3.5 cursor-pointer accent-orange"
                  />
                  {spec.displayName} CLI config
                </label>
              ) : null}
            </div>
          </fieldset>

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
          <strong className="text-ink">{spec.displayName} CLI:</strong> you'll need to log in once.{' '}
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
