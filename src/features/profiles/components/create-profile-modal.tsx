import type { Dependencies, Surfaces } from '@/lib/types'

import { useState } from 'react'

import { Button } from '@/design/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/design/ui/dialog'
import { Input } from '@/design/ui/input'
import { Label } from '@/design/ui/label'
import { isValidHexColor, presetColors } from '@/lib/colors'

import { ColorSwatchPicker } from './color-swatch-picker'

type Props = {
  open: boolean
  dependencies: Dependencies
  submitting?: boolean
  onClose: () => void
  onCreate: (input: { name: string; color: string; surfaces: Surfaces }) => Promise<void>
}

export function CreateProfileModal({ open, dependencies, submitting, onClose, onCreate }: Props) {
  const [name, setName] = useState('')
  const [color, setColor] = useState<string>(presetColors[0])
  const [gui, setGui] = useState(true)
  const [cli, setCli] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const effectiveGui = gui && dependencies.claudeAppInstalled
  const effectiveCli = cli && dependencies.claudeCliInstalled
  const canSubmit = name.trim().length > 0 && isValidHexColor(color) && (effectiveGui || effectiveCli)

  async function handleSubmit() {
    setError(null)
    try {
      await onCreate({
        name: name.trim(),
        color,
        surfaces: { gui: effectiveGui, cli: effectiveCli },
      })
      setName('')
      setColor(presetColors[0])
      setGui(true)
      setCli(true)
      onClose()
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    }
  }

  return (
    <Dialog open={open} onOpenChange={(next) => (next ? null : onClose())}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>New profile</DialogTitle>
          <DialogDescription>
            A profile bundles a desktop app launcher and a CLI wrapper. Pick a name and color.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4 py-2">
          <div className="space-y-2">
            <Label htmlFor="name">Name</Label>
            <Input
              id="name"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="Personal"
              autoFocus
            />
          </div>
          <ColorSwatchPicker value={color} onChange={setColor} />
          <fieldset className="space-y-2">
            <legend className="text-sm font-medium">Surfaces</legend>
            <label
              className={
                dependencies.claudeAppInstalled
                  ? 'flex items-center gap-2 text-sm'
                  : 'flex items-center gap-2 text-sm text-muted-foreground'
              }
            >
              <input
                type="checkbox"
                checked={gui && dependencies.claudeAppInstalled}
                disabled={!dependencies.claudeAppInstalled}
                onChange={(event) => setGui(event.target.checked)}
              />
              Desktop app launcher (in /Applications)
            </label>
            {!dependencies.claudeAppInstalled ? (
              <p className="pl-6 text-xs text-muted-foreground">
                Install{' '}
                <a className="underline" href="https://claude.ai/download" target="_blank" rel="noreferrer">
                  Claude Desktop
                </a>{' '}
                first.
              </p>
            ) : null}
            <label
              className={
                dependencies.claudeCliInstalled
                  ? 'flex items-center gap-2 text-sm'
                  : 'flex items-center gap-2 text-sm text-muted-foreground'
              }
            >
              <input
                type="checkbox"
                checked={cli && dependencies.claudeCliInstalled}
                disabled={!dependencies.claudeCliInstalled}
                onChange={(event) => setCli(event.target.checked)}
              />
              Claude Code CLI wrapper (in ~/.local/bin)
            </label>
            {!dependencies.claudeCliInstalled ? (
              <p className="pl-6 text-xs text-muted-foreground">
                Install Claude Code first: <code>npm install -g @anthropic-ai/claude-code</code>
              </p>
            ) : null}
          </fieldset>
          {error ? <p className="text-sm text-red">{error}</p> : null}
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose} disabled={submitting}>
            Cancel
          </Button>
          <Button onClick={handleSubmit} disabled={!canSubmit || submitting}>
            Create
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
