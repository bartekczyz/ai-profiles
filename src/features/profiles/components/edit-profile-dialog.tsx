import type { Dependencies, Profile, Surfaces } from '@/lib/types'

import { useEffect, useState } from 'react'

import { Button, Dialog, Kbd, useToast } from '@/design'
import { isValidHexColor } from '@/lib/colors'
import { extractErrorMessage } from '@/lib/extract-error-message'

import { ProfileFormFields } from './profile-form-fields'

type Props = {
  open: boolean
  profile: Profile
  dependencies: Dependencies
  submitting?: boolean
  onClose: () => void
  onSave: (input: { name: string; color: string; surfaces: Surfaces }) => Promise<void>
}

export function EditProfileDialog({ open, profile, dependencies, submitting, onClose, onSave }: Props) {
  const toast = useToast()
  const [name, setName] = useState(profile.name)
  const [color, setColor] = useState(profile.color)
  const [surfaces, setSurfaces] = useState<Surfaces>(profile.surfaces)

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset only when the profile identity changes
  useEffect(() => {
    setName(profile.name)
    setColor(profile.color)
    setSurfaces(profile.surfaces)
  }, [profile.id])

  const dirty =
    name.trim() !== profile.name ||
    color.toLowerCase() !== profile.color.toLowerCase() ||
    surfaces.gui !== profile.surfaces.gui ||
    surfaces.cli !== profile.surfaces.cli
  const canSubmit = name.trim().length > 0 && isValidHexColor(color) && dirty && (surfaces.gui || surfaces.cli)

  async function handleSubmit() {
    if (!canSubmit || submitting) {
      return
    }
    try {
      await onSave({ name: name.trim(), color, surfaces })
      onClose()
    } catch (caught) {
      toast.error('Could not save profile.', extractErrorMessage(caught))
    }
  }

  // Slug is derived from name and persists per-profile; the live preview
  // only matters while name is changing. Hide it when the name is
  // unchanged so the edit dialog reads as a small tweak, not a rename.
  const showSlugPreview = name.trim() !== profile.name

  return (
    <Dialog
      open={open}
      title="Edit profile"
      description="Rename, repaint, or toggle surfaces. Existing data on disk stays put."
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
            Save
          </Button>
        </>
      }
    >
      <ProfileFormFields
        name={name}
        color={color}
        surfaces={surfaces}
        dependencies={dependencies}
        showSlugPreview={showSlugPreview}
        onNameChange={setName}
        onColorChange={setColor}
        onSurfacesChange={setSurfaces}
      />
    </Dialog>
  )
}
