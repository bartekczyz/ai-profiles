import type { DefaultEntry } from '@/lib/types'
import type { DefaultProfileEdit } from '../lib/default-profile-edit'

import { useEffect, useState } from 'react'

import { Button, Dialog, Kbd, useToast } from '@/design'
import { Input } from '@/design/ui/input'
import { appSpecs } from '@/lib/app-registry'
import { extractErrorMessage } from '@/lib/extract-error-message'

import { planDefaultProfileEdit, resetDefaultProfileEdit } from '../lib/default-profile-edit'
import { ColorSwatchPicker } from './color-swatch-picker'

type Props = {
  open: boolean
  entry: DefaultEntry
  onClose: () => void
  onSave: (edit: DefaultProfileEdit) => Promise<void>
}

const maxLength = 64

const labelClasses = 'font-mono text-[11.5px] font-medium uppercase tracking-[0.08em] text-muted'

/**
 * Names and colours the stock-install entry. Both are labels only — the stock
 * app, its data directory and the plain CLI binary are untouched — so this is
 * two fields rather than the managed profile's Edit dialog. Without a colour
 * the entry keeps its app's brand mark.
 */
export function EditDefaultProfileDialog({ open, entry, onClose, onSave }: Props) {
  const toast = useToast()
  const [name, setName] = useState(entry.customName ?? '')
  const [color, setColor] = useState(entry.color ?? '')
  const [saving, setSaving] = useState(false)

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset each time the dialog opens
  useEffect(() => {
    if (open) {
      setName(entry.customName ?? '')
      setColor(entry.color ?? '')
    }
  }, [open])

  const planned = planDefaultProfileEdit(entry, name, color)
  const canSubmit = planned.valid && !saving

  async function save(edit: DefaultProfileEdit) {
    setSaving(true)
    try {
      await onSave(edit)
      onClose()
    } catch (caught) {
      toast.error('Could not save the default profile.', extractErrorMessage(caught))
    } finally {
      setSaving(false)
    }
  }

  async function handleSubmit() {
    if (canSubmit) {
      await save(planned.edit)
    }
  }

  return (
    <Dialog
      open={open}
      title="Edit default profile"
      description={`Only its label and colour change. The stock ${appSpecs[entry.app].displayName} install is left as it is.`}
      onClose={onClose}
      onSubmit={handleSubmit}
      foot={
        <EditDefaultProfileFoot
          customized={entry.customName !== null || entry.color !== null}
          saving={saving}
          canSubmit={canSubmit}
          onReset={() => save(resetDefaultProfileEdit(entry))}
          onCancel={onClose}
          onSubmit={handleSubmit}
        />
      }
    >
      <div className="space-y-4">
        <div>
          <label htmlFor="default-profile-name" className={`mb-1.5 block ${labelClasses}`}>
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
        </div>
        <ColorField color={color} onColorChange={setColor} />
      </div>
    </Dialog>
  )
}

type EditDefaultProfileFootProps = {
  /**
   * Whether the entry has a name or colour of its own, which Reset takes off.
   */
  customized: boolean
  /**
   * Whether a save is under way.
   */
  saving: boolean
  /**
   * Whether the form has something valid to save.
   */
  canSubmit: boolean
  /**
   * Puts the stock label back, with no colour.
   */
  onReset: () => void
  /**
   * Closes without saving.
   */
  onCancel: () => void
  /**
   * Saves the form.
   */
  onSubmit: () => void
}

/**
 * The dialog's buttons: Reset on the left while there is something to reset,
 * then Cancel and Save.
 */
function EditDefaultProfileFoot({
  customized,
  saving,
  canSubmit,
  onReset,
  onCancel,
  onSubmit,
}: EditDefaultProfileFootProps) {
  return (
    <>
      {customized ? (
        <Button variant="ghost" size="sm" disabled={saving} className="mr-auto" onClick={onReset}>
          Reset
        </Button>
      ) : null}
      <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={saving} onClick={onCancel}>
        Cancel
      </Button>
      <Button
        variant="primary"
        size="sm"
        trailingKbd={<Kbd variant="onOrange">⏎</Kbd>}
        disabled={!canSubmit}
        onClick={onSubmit}
      >
        {saving ? 'Saving…' : 'Save'}
      </Button>
    </>
  )
}

type ColorFieldProps = {
  /**
   * The colour as typed or picked, `''` for none.
   */
  color: string
  /**
   * Takes the new colour, `''` for none.
   */
  onColorChange: (color: string) => void
}

/**
 * The colour picker, with a way back to no colour once one is set.
 */
function ColorField({ color, onColorChange }: ColorFieldProps) {
  return (
    <div>
      <div className="mb-1.5 flex items-baseline justify-between">
        <span className={labelClasses}>Color</span>
        {color.trim() !== '' ? (
          <button
            type="button"
            className="cursor-pointer text-meta text-muted-strong hover:text-ink"
            onClick={() => onColorChange('')}
          >
            No color
          </button>
        ) : null}
      </div>
      <ColorSwatchPicker value={color} onChange={onColorChange} />
    </div>
  )
}
