import type { AppId, Dependencies, Surfaces } from '@/lib/types'

import { useState } from 'react'

import { Button, Dialog, Kbd, useToast } from '@/design'
import { appIds } from '@/lib/app-registry'
import { isValidHexColor, presetColors } from '@/lib/colors'
import { extractErrorMessage } from '@/lib/extract-error-message'

import { ProfileFormFields } from './profile-form-fields'

type Props = {
  open: boolean
  dependencies: Dependencies
  submitting?: boolean
  onClose: () => void
  onCreate: (input: { app: AppId; name: string; color: string; surfaces: Surfaces }) => Promise<void>
}

export function CreateProfileDialog({ open, dependencies, submitting, onClose, onCreate }: Props) {
  const toast = useToast()
  const [name, setName] = useState('')
  const [color, setColor] = useState<string>(presetColors[0])
  const [surfaces, setSurfaces] = useState<Surfaces>({ gui: true, cli: true })

  const installedApps = appIds.filter((id) => dependencies.apps[id].guiInstalled || dependencies.apps[id].cliInstalled)
  // Pre-select when exactly one app is installed; otherwise leave empty so the
  // user makes a deliberate choice.
  const defaultApp: AppId | '' = installedApps.length === 1 ? installedApps[0] : ''
  const [app, setApp] = useState<AppId | ''>(defaultApp)

  const appDeps = app !== '' ? dependencies.apps[app] : null
  const effectiveGui = surfaces.gui && (appDeps?.guiInstalled ?? false)
  const effectiveCli = surfaces.cli && (appDeps?.cliInstalled ?? false)
  const canSubmit = app !== '' && name.trim().length > 0 && isValidHexColor(color) && (effectiveGui || effectiveCli)

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
        surfaces: { gui: effectiveGui, cli: effectiveCli },
      })
      setName('')
      setColor(presetColors[0])
      setSurfaces({ gui: true, cli: true })
      setApp(defaultApp)
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
        app={app}
        name={name}
        color={color}
        surfaces={surfaces}
        dependencies={dependencies}
        installedApps={installedApps}
        onAppChange={setApp}
        onNameChange={setName}
        onColorChange={setColor}
        onSurfacesChange={setSurfaces}
      />
    </Dialog>
  )
}
