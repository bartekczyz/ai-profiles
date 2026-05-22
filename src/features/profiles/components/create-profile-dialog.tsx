import type { Dependencies, Surfaces } from '@/lib/types'

import { useState } from 'react'

import { Button, Dialog, Kbd, useToast } from '@/design'
import { isValidHexColor, presetColors } from '@/lib/colors'
import { extractErrorMessage } from '@/lib/extract-error-message'

import { ProfileFormFields } from './profile-form-fields'

type Props = {
  open: boolean
  dependencies: Dependencies
  submitting?: boolean
  onClose: () => void
  onCreate: (input: { name: string; color: string; surfaces: Surfaces }) => Promise<void>
}

export function CreateProfileDialog({ open, dependencies, submitting, onClose, onCreate }: Props) {
  const toast = useToast()
  const [name, setName] = useState('')
  const [color, setColor] = useState<string>(presetColors[0])
  const [surfaces, setSurfaces] = useState<Surfaces>({ gui: true, cli: true })

  const effectiveGui = surfaces.gui && dependencies.claudeAppInstalled
  const effectiveCli = surfaces.cli && dependencies.claudeCliInstalled
  const canSubmit = name.trim().length > 0 && isValidHexColor(color) && (effectiveGui || effectiveCli)

  async function handleSubmit() {
    if (!canSubmit || submitting) {
      return
    }
    try {
      await onCreate({
        name: name.trim(),
        color,
        surfaces: { gui: effectiveGui, cli: effectiveCli },
      })
      setName('')
      setColor(presetColors[0])
      setSurfaces({ gui: true, cli: true })
      onClose()
    } catch (caught) {
      toast.error('Could not create profile.', extractErrorMessage(caught))
    }
  }

  return (
    <Dialog
      open={open}
      title="New profile"
      description="A profile bundles a Desktop launcher and a CLI wrapper. Pick a name and color; everything else stays isolated."
      onClose={onClose}
      onSubmit={handleSubmit}
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={submitting} onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            size="sm"
            trailingKbd={<Kbd variant="onOrange">⏎</Kbd>}
            disabled={!canSubmit || submitting}
            onClick={handleSubmit}
          >
            Create profile
          </Button>
        </>
      }
    >
      <ProfileFormFields
        name={name}
        color={color}
        surfaces={surfaces}
        dependencies={dependencies}
        onNameChange={setName}
        onColorChange={setColor}
        onSurfacesChange={setSurfaces}
      />
    </Dialog>
  )
}
