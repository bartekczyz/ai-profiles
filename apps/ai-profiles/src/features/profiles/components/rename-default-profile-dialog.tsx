import type { DefaultEntry } from '@/lib/types'

import { useEffect, useState } from 'react'

import { Button, Dialog, Kbd, useToast } from '@/design'
import { Input } from '@/design/ui/input'
import { appSpecs } from '@/lib/app-registry'
import { extractErrorMessage } from '@/lib/extract-error-message'

type Props = {
  open: boolean
  entry: DefaultEntry
  onClose: () => void
  /** An empty name restores the stock label. */
  onSave: (name: string) => Promise<void>
}

const maxLength = 64

/**
 * Renames the stock-install entry. The name is a label only — the stock app,
 * its data directory and the plain CLI binary are untouched — so this is a
 * single field rather than the managed profile's Edit dialog.
 */
export function RenameDefaultProfileDialog({ open, entry, onClose, onSave }: Props) {
  const toast = useToast()
  const [name, setName] = useState(entry.customName ?? '')
  const [saving, setSaving] = useState(false)

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset each time the dialog opens
  useEffect(() => {
    if (open) {
      setName(entry.customName ?? '')
    }
  }, [open])

  const trimmed = name.trim()
  const dirty = trimmed !== (entry.customName ?? '')
  const canSubmit = dirty && !saving

  async function save(next: string) {
    setSaving(true)
    try {
      await onSave(next)
      onClose()
    } catch (caught) {
      toast.error('Could not rename profile.', extractErrorMessage(caught))
    } finally {
      setSaving(false)
    }
  }

  async function handleSubmit() {
    if (canSubmit) {
      await save(trimmed)
    }
  }

  return (
    <Dialog
      open={open}
      title="Rename default profile"
      description={`Only the label changes. The stock ${appSpecs[entry.app].displayName} install is left as it is.`}
      onClose={onClose}
      onSubmit={handleSubmit}
      foot={
        <>
          {entry.customName !== null ? (
            <Button variant="ghost" size="sm" disabled={saving} className="mr-auto" onClick={() => save('')}>
              Reset name
            </Button>
          ) : null}
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={saving} onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            size="sm"
            trailingKbd={<Kbd variant="onOrange">⏎</Kbd>}
            disabled={!canSubmit}
            onClick={handleSubmit}
          >
            {saving ? 'Saving…' : 'Save'}
          </Button>
        </>
      }
    >
      <label
        htmlFor="default-profile-name"
        className="mb-1.5 block font-mono text-[11.5px] font-medium uppercase tracking-[0.08em] text-muted"
      >
        Name
      </label>
      <Input
        autoFocus
        id="default-profile-name"
        type="text"
        value={name}
        maxLength={maxLength}
        onChange={(event) => setName(event.target.value)}
        placeholder={appSpecs[entry.app].displayName}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
      />
    </Dialog>
  )
}
