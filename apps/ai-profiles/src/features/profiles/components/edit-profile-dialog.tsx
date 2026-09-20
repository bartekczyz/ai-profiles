import type { Dependencies, Profile, Surfaces } from '@/lib/types'

import { useEffect, useState } from 'react'

import { Button, Dialog, Kbd, useToast } from '@/design'
import { isValidHexColor } from '@/lib/colors'
import { extractErrorMessage } from '@/lib/extract-error-message'

import { DockIconConsentDialog } from './dock-icon-consent-dialog'
import { ProfileFormFields } from './profile-form-fields'
import { useDockIconConsent } from './use-dock-icon-consent'

type Props = {
  open: boolean
  profile: Profile
  dependencies: Dependencies
  /**
   * Whether the user has already confirmed they understand what a Dock icon of
   * its own involves. Until they have, turning it on explains itself first.
   */
  dockIconAcknowledged: boolean
  submitting?: boolean
  onClose: () => void
  onAcknowledgeDockIcon: () => Promise<void>
  onSave: (input: { name: string; color: string; surfaces: Surfaces; distinctDockIcon: boolean }) => Promise<void>
}

export function EditProfileDialog({
  open,
  profile,
  dependencies,
  dockIconAcknowledged,
  submitting,
  onClose,
  onAcknowledgeDockIcon,
  onSave,
}: Props) {
  const toast = useToast()
  const [name, setName] = useState(profile.name)
  const [color, setColor] = useState(profile.color)
  const [surfaces, setSurfaces] = useState<Surfaces>(profile.surfaces)
  const [distinctDockIcon, setDistinctDockIcon] = useState(profile.distinctDockIcon)

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset only when the profile identity changes
  useEffect(() => {
    setName(profile.name)
    setColor(profile.color)
    setSurfaces(profile.surfaces)
    setDistinctDockIcon(profile.distinctDockIcon)
  }, [profile.id])

  const dockIconConsent = useDockIconConsent({
    acknowledged: dockIconAcknowledged,
    onChoose: setDistinctDockIcon,
    onAcknowledge: onAcknowledgeDockIcon,
  })

  const dirty =
    name.trim() !== profile.name ||
    color.toLowerCase() !== profile.color.toLowerCase() ||
    surfaces.gui !== profile.surfaces.gui ||
    surfaces.cli !== profile.surfaces.cli ||
    distinctDockIcon !== profile.distinctDockIcon
  const canSubmit = name.trim().length > 0 && isValidHexColor(color) && dirty && (surfaces.gui || surfaces.cli)

  async function handleSubmit() {
    if (!canSubmit || submitting) {
      return
    }
    try {
      await onSave({ name: name.trim(), color, surfaces, distinctDockIcon })
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
    <>
      <Dialog
        open={open}
        title="Edit profile"
        description="Rename, repaint, or toggle surfaces. Existing data on disk stays put."
        closeOnOutsideClick={false}
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
              {submitting ? 'Saving…' : 'Save'}
            </Button>
          </>
        }
      >
        <ProfileFormFields
          app={profile.app}
          name={name}
          color={color}
          surfaces={surfaces}
          distinctDockIcon={distinctDockIcon}
          dependencies={dependencies}
          showSlugPreview={showSlugPreview}
          onNameChange={setName}
          onColorChange={setColor}
          onSurfacesChange={setSurfaces}
          onDistinctDockIconChange={dockIconConsent.choose}
          onExplainDockIcon={dockIconConsent.explain}
        />
      </Dialog>
      {/* A sibling rather than a child, so keys pressed in it are not taken for
          keys pressed in the form underneath. */}
      <DockIconConsentDialog
        open={dockIconConsent.open}
        app={profile.app}
        onClose={dockIconConsent.cancel}
        onConfirm={dockIconConsent.asking ? dockIconConsent.confirm : undefined}
      />
    </>
  )
}
