import type { Dependencies, Profile, Surfaces } from '@/lib/types'
import type { IconChange } from '../lib/launcher-icon'

import { useEffect, useState } from 'react'

import { Dialog, useToast } from '@/design'
import { extractErrorMessage } from '@/lib/extract-error-message'

import { changesAnything, planProfileEdit } from '../lib/plan-profile-edit'
import { isProfileFormValid } from '../lib/profile-form'
import { DockIconConsentDialog } from './dock-icon-consent-dialog'
import { useLauncherIconChange } from './launcher-icon-controls'
import { ProfileDialogFoot } from './profile-dialog-foot'
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
  /**
   * The image the profile's launcher shows in place of the generated icon, as
   * a data URL, or `null` for none; absent until that is known.
   */
  customIcon?: string | null
  submitting?: boolean
  onClose: () => void
  onAcknowledgeDockIcon: () => Promise<void>
  onSave: (input: {
    name: string
    color: string
    surfaces: Surfaces
    distinctDockIcon: boolean
    /**
     * A change to the launcher's icon, or `null` to leave it as it is.
     */
    iconChange: IconChange | null
  }) => Promise<void>
}

export function EditProfileDialog({
  open,
  profile,
  dependencies,
  dockIconAcknowledged,
  customIcon,
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
  const launcherIcon = useLauncherIconChange({ profileId: profile.id, app: profile.app, color, customIcon, open })

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

  // Dirty when saving would change anything: the same diff the save itself runs.
  const plan = planProfileEdit(profile, { name: name.trim(), color, surfaces, distinctDockIcon })
  const canSubmit = changesAnything(plan, launcherIcon.change) && isProfileFormValid(name, color, surfaces)

  // An image picked but not saved goes with the dialog, so it can't come back
  // looking applied when the dialog is opened again.
  function close() {
    launcherIcon.clear()
    onClose()
  }

  async function handleSubmit() {
    if (!canSubmit || submitting) {
      return
    }
    try {
      await onSave({ name: name.trim(), color, surfaces, distinctDockIcon, iconChange: launcherIcon.change })
      close()
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
        onClose={close}
        onSubmit={handleSubmit}
        foot={
          <ProfileDialogFoot
            canSubmit={canSubmit}
            submitting={submitting}
            submitLabel="Save"
            submittingLabel="Saving…"
            onCancel={close}
            onSubmit={handleSubmit}
          />
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
          launcherIcon={launcherIcon.field}
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
