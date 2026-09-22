import type { DefaultEntry } from '@/lib/types'

import { useEffect, useState } from 'react'

import { Button, Dialog, Kbd, useToast } from '@/design'
import { Input } from '@/design/ui/input'
import { appSpecs } from '@/lib/app-registry'
import { isValidHexColor } from '@/lib/colors'
import { extractErrorMessage } from '@/lib/extract-error-message'

import { ColorSwatchPicker } from './color-swatch-picker'

/**
 * What changed. A field left out is left as it is; an empty string puts the
 * stock label, or no colour, back.
 */
export type DefaultProfileEdit = {
  name?: string
  color?: string
}

type Props = {
  open: boolean
  entry: DefaultEntry
  onClose: () => void
  onSave: (edit: DefaultProfileEdit) => Promise<void>
}

const maxLength = 64

const labelClasses = 'mb-1.5 block font-mono text-[11.5px] font-medium uppercase tracking-[0.08em] text-muted'

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

  const trimmedName = name.trim()
  const trimmedColor = color.trim().toLowerCase()
  const nameChanged = trimmedName !== (entry.customName ?? '')
  const colorChanged = trimmedColor !== (entry.color ?? '')
  const colorValid = trimmedColor === '' || isValidHexColor(trimmedColor)
  const canSubmit = (nameChanged || colorChanged) && colorValid && !saving
  const customized = entry.customName !== null || entry.color !== null

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
      await save({
        ...(nameChanged ? { name: trimmedName } : {}),
        ...(colorChanged ? { color: trimmedColor } : {}),
      })
    }
  }

  function handleReset() {
    return save({
      ...(entry.customName !== null ? { name: '' } : {}),
      ...(entry.color !== null ? { color: '' } : {}),
    })
  }

  return (
    <Dialog
      open={open}
      title="Edit default profile"
      description={`Only its label and colour change. The stock ${appSpecs[entry.app].displayName} install is left as it is.`}
      onClose={onClose}
      onSubmit={handleSubmit}
      foot={
        <>
          {customized ? (
            <Button variant="ghost" size="sm" disabled={saving} className="mr-auto" onClick={handleReset}>
              Reset
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
      <div className="space-y-4">
        <div>
          <label htmlFor="default-profile-name" className={labelClasses}>
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
        <div>
          <div className="mb-1.5 flex items-baseline justify-between">
            <span className={labelClasses.replace('mb-1.5 ', '')}>Color</span>
            {trimmedColor !== '' ? (
              <button
                type="button"
                className="cursor-pointer text-meta text-muted-strong hover:text-ink"
                onClick={() => setColor('')}
              >
                No color
              </button>
            ) : null}
          </div>
          <ColorSwatchPicker value={color} onChange={setColor} />
        </div>
      </div>
    </Dialog>
  )
}
