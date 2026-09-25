import type { AppId, Dependencies, Surfaces } from '@/lib/types'

import { useState } from 'react'

import { Dialog, useToast } from '@/design'
import { presetColors } from '@/lib/colors'
import { extractErrorMessage } from '@/lib/extract-error-message'

import {
  availableSurfaces,
  effectiveSurfaces,
  installedAppIds,
  isProfileFormValid,
  newProfileDockIcon,
  preselectedApp,
} from '../lib/profile-form'
import { DockIconConsentDialog } from './dock-icon-consent-dialog'
import { ProfileDialogFoot } from './profile-dialog-foot'
import { ProfileFormFields } from './profile-form-fields'
import { useDockIconConsent } from './use-dock-icon-consent'

type Props = {
  open: boolean
  dependencies: Dependencies
  /**
   * Whether the user has already confirmed they understand what a Dock icon of
   * its own involves. Until they have, the option starts off for every app and
   * turning it on explains itself first; after, it starts on for the apps where
   * that costs nothing they would notice.
   */
  dockIconAcknowledged: boolean
  submitting?: boolean
  onClose: () => void
  onAcknowledgeDockIcon: () => Promise<void>
  onCreate: (input: {
    app: AppId
    name: string
    color: string
    surfaces: Surfaces
    distinctDockIcon: boolean
  }) => Promise<void>
}

export function CreateProfileDialog({
  open,
  dependencies,
  dockIconAcknowledged,
  submitting,
  onClose,
  onAcknowledgeDockIcon,
  onCreate,
}: Props) {
  const toast = useToast()
  const [name, setName] = useState('')
  const [color, setColor] = useState<string>(presetColors[0])
  const [surfaces, setSurfaces] = useState<Surfaces>({ gui: true, cli: true })
  // `null` until the user has chosen, so that the app's default follows them
  // when they change the app.
  const [dockIconChoice, setDockIconChoice] = useState<boolean | null>(null)

  const installedApps = installedAppIds(dependencies)
  const defaultApp = preselectedApp(installedApps)
  const [app, setApp] = useState<AppId | ''>(defaultApp)

  const effective = effectiveSurfaces(surfaces, availableSurfaces(dependencies, app))
  const canSubmit = app !== '' && isProfileFormValid(name, color, effective)

  const dockIcon = newProfileDockIcon(dockIconChoice, app, dockIconAcknowledged, effective.gui)
  const dockIconConsent = useDockIconConsent({
    acknowledged: dockIconAcknowledged,
    onChoose: setDockIconChoice,
    onAcknowledge: onAcknowledgeDockIcon,
  })

  async function handleSubmit() {
    if (!canSubmit || submitting) {
      return
    }
    // canSubmit guarantees app !== '', so cast is safe
    const selectedApp = app as AppId
    try {
      await onCreate({
        app: selectedApp,
        name: name.trim(),
        color,
        surfaces: effective,
        distinctDockIcon: dockIcon,
      })
      setName('')
      setColor(presetColors[0])
      setSurfaces({ gui: true, cli: true })
      setDockIconChoice(null)
      setApp(defaultApp)
      onClose()
    } catch (caught) {
      toast.error('Could not create profile.', extractErrorMessage(caught))
    }
  }

  return (
    <>
      <Dialog
        open={open}
        title="New profile"
        description="A profile bundles a Desktop launcher and a CLI wrapper. Pick a name and color; everything else stays isolated."
        closeOnOutsideClick={false}
        onClose={onClose}
        onSubmit={handleSubmit}
        foot={
          <ProfileDialogFoot
            canSubmit={canSubmit}
            submitting={submitting}
            submitLabel="Create profile"
            submittingLabel="Creating…"
            onCancel={onClose}
            onSubmit={handleSubmit}
          />
        }
      >
        <ProfileFormFields
          app={app}
          name={name}
          color={color}
          surfaces={surfaces}
          distinctDockIcon={dockIcon}
          dependencies={dependencies}
          installedApps={installedApps}
          onAppChange={setApp}
          onNameChange={setName}
          onColorChange={setColor}
          onSurfacesChange={setSurfaces}
          onDistinctDockIconChange={dockIconConsent.choose}
          onExplainDockIcon={dockIconConsent.explain}
        />
      </Dialog>
      {/* A sibling rather than a child, so keys pressed in it are not taken for
          keys pressed in the form underneath. */}
      {app !== '' ? (
        <DockIconConsentDialog
          open={dockIconConsent.open}
          app={app}
          onClose={dockIconConsent.cancel}
          onConfirm={dockIconConsent.asking ? dockIconConsent.confirm : undefined}
        />
      ) : null}
    </>
  )
}
