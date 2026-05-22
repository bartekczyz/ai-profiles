import type { ExistingInstallInfo, ImportExistingInput, Profile } from '@/lib/types'

import { useState } from 'react'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { isValidHexColor, presetColors } from '@/lib/colors'

import { ColorSwatchPicker } from './color-swatch-picker'

type Props = {
  open: boolean
  existing: ExistingInstallInfo
  onClose: () => void
  onImport: (input: ImportExistingInput) => Promise<Profile>
}

export function MigrationDialog({ open, existing, onClose, onImport }: Props) {
  const [name, setName] = useState('Default')
  const [color, setColor] = useState<string>(presetColors[0])
  const [includeGui, setIncludeGui] = useState(existing.claudeDesktopPath !== null)
  const [includeCli, setIncludeCli] = useState(existing.claudeCodePath !== null)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const canSubmit = name.trim().length > 0 && isValidHexColor(color) && (includeGui || includeCli) && !submitting

  async function handleSubmit() {
    setSubmitting(true)
    setError(null)
    try {
      await onImport({
        name: name.trim(),
        color,
        includeGui,
        includeCli,
      })
      onClose()
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(next) => (next ? null : onClose())}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Import existing Claude install</DialogTitle>
          <DialogDescription>
            We found an existing Claude install on this Mac. Import it as a named profile so you don&apos;t lose
            history.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4 py-2">
          <ul className="rounded-md border border-[color:var(--color-border)] p-3 text-xs">
            {existing.claudeDesktopPath ? (
              <li>
                <span className="font-semibold">Claude Desktop:</span> <code>{existing.claudeDesktopPath}</code>
              </li>
            ) : null}
            {existing.claudeCodePath ? (
              <li className="mt-1">
                <span className="font-semibold">Claude Code CLI:</span> <code>{existing.claudeCodePath}</code>
              </li>
            ) : null}
          </ul>
          <div className="space-y-2">
            <Label htmlFor="migration-name">Profile name</Label>
            <Input id="migration-name" value={name} onChange={(event) => setName(event.target.value)} autoFocus />
          </div>
          <ColorSwatchPicker value={color} onChange={setColor} />
          <fieldset className="space-y-2">
            <legend className="text-sm font-medium">What to import</legend>
            {existing.claudeDesktopPath ? (
              <label className="flex items-center gap-2 text-sm">
                <input type="checkbox" checked={includeGui} onChange={(event) => setIncludeGui(event.target.checked)} />
                Desktop app data (history, login)
              </label>
            ) : null}
            {existing.claudeCodePath ? (
              <label className="flex items-center gap-2 text-sm">
                <input type="checkbox" checked={includeCli} onChange={(event) => setIncludeCli(event.target.checked)} />
                Claude Code CLI config
              </label>
            ) : null}
          </fieldset>
          {includeCli ? (
            <p className="rounded-md bg-amber-50 px-3 py-2 text-xs text-amber-900">
              Heads up: you&apos;ll need to log in to Claude Code once after importing. macOS Keychain keys are derived
              from <code>CLAUDE_CONFIG_DIR</code>, so the existing credentials don&apos;t carry over.
            </p>
          ) : null}
          <p className="text-xs text-muted-foreground">
            We&apos;ll move your existing data into this profile and keep a one-shot backup under{' '}
            <code>~/Library/Application Support/claude-profiles/migration-backup-…/</code>. You can delete the backup
            from Settings after 7 days.
          </p>
          {error ? <p className="text-sm text-red">{error}</p> : null}
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose} disabled={submitting}>
            Skip for now
          </Button>
          <Button onClick={handleSubmit} disabled={!canSubmit}>
            Import
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
