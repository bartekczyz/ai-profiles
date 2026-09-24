import type { AppId, Dependencies, RemoteHost, Surfaces } from '@/lib/types'

import { useState } from 'react'

import { Button, Dialog, Kbd, useToast } from '@/design'
// cross-feature: a remote profile's name has to be free on its server
import { useRemoteAccounts } from '@/features/remote/api/use-remote'
import { appIds, appSpecs } from '@/lib/app-registry'
import { isValidHexColor, presetColors } from '@/lib/colors'
import { extractErrorMessage } from '@/lib/extract-error-message'

import { DockIconConsentDialog } from './dock-icon-consent-dialog'
import { isValidRemoteProfileName, ProfileFormFields, type ProfileType, remoteType } from './profile-form-fields'
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
  /** Paired servers, where a Claude CLI Remote profile can be made. */
  remoteHosts?: Array<RemoteHost>
  /** Open on the remote type, on this server. */
  initialRemoteHostId?: string
  /** Make a profile on a server. Resolves once it exists there. */
  onCreateRemote?: (input: { hostId: string; name: string; color: string }) => Promise<void>
}

export function CreateProfileDialog({
  open,
  dependencies,
  dockIconAcknowledged,
  submitting,
  onClose,
  onAcknowledgeDockIcon,
  onCreate,
  remoteHosts = [],
  initialRemoteHostId,
  onCreateRemote,
}: Props) {
  const toast = useToast()
  const [name, setName] = useState('')
  const [color, setColor] = useState<string>(presetColors[0])
  const [surfaces, setSurfaces] = useState<Surfaces>({ gui: true, cli: true })
  // `null` until the user has chosen, so that the app's default follows them
  // when they change the app.
  const [dockIconChoice, setDockIconChoice] = useState<boolean | null>(null)

  const installedApps = appIds.filter((id) => dependencies.apps[id].guiInstalled || dependencies.apps[id].cliInstalled)
  // Pre-select when exactly one app is installed; otherwise leave empty so the
  // user makes a deliberate choice.
  const defaultApp: ProfileType =
    initialRemoteHostId !== undefined ? remoteType : installedApps.length === 1 ? installedApps[0] : ''
  const [app, setApp] = useState<ProfileType>(defaultApp)
  const [hostId, setHostId] = useState<string>(initialRemoteHostId ?? remoteHosts[0]?.id ?? '')
  const remote = app === remoteType
  // Names on the chosen server, to say before creating that one is taken.
  const serverProfiles = useRemoteAccounts(hostId, remote && hostId !== '')
  const takenOnServer =
    remote && (serverProfiles.data ?? []).some((account) => account.name.toLowerCase() === name.trim().toLowerCase())

  const appDeps = app !== '' && !remote ? dependencies.apps[app] : null
  const effectiveGui = surfaces.gui && (appDeps?.guiInstalled ?? false)
  const effectiveCli = surfaces.cli && (appDeps?.cliInstalled ?? false)
  const canSubmit = remote
    ? remoteHosts.some((host) => host.id === hostId) &&
      isValidRemoteProfileName(name.trim()) &&
      !takenOnServer &&
      isValidHexColor(color)
    : app !== '' && name.trim().length > 0 && isValidHexColor(color) && (effectiveGui || effectiveCli)

  const dockIconByDefault = app !== '' && !remote && dockIconAcknowledged && appSpecs[app].dockIcon.defaultOn
  const dockIcon = (dockIconChoice ?? dockIconByDefault) && effectiveGui
  const dockIconConsent = useDockIconConsent({
    acknowledged: dockIconAcknowledged,
    onChoose: setDockIconChoice,
    onAcknowledge: onAcknowledgeDockIcon,
  })

  async function handleSubmit() {
    if (!canSubmit || submitting) {
      return
    }
    if (remote) {
      try {
        await onCreateRemote?.({ hostId, name: name.trim(), color })
        setName('')
        setColor(presetColors[0])
        setApp(defaultApp)
        onClose()
      } catch (caught) {
        toast.error('Could not create profile.', extractErrorMessage(caught))
      }
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
        description={
          remote
            ? 'A profile on a server: Claude runs there, in tmux, with Remote Control, signed in as its own account.'
            : 'A profile bundles a Desktop launcher and a CLI wrapper. Pick a name and color; everything else stays isolated.'
        }
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
              {submitting ? 'Creating…' : 'Create profile'}
            </Button>
          </>
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
          remote={
            onCreateRemote ? { hosts: remoteHosts, hostId, onHostChange: setHostId, taken: takenOnServer } : undefined
          }
          onNameChange={setName}
          onColorChange={setColor}
          onSurfacesChange={setSurfaces}
          onDistinctDockIconChange={dockIconConsent.choose}
          onExplainDockIcon={dockIconConsent.explain}
        />
      </Dialog>
      {/* A sibling rather than a child, so keys pressed in it are not taken for
          keys pressed in the form underneath. */}
      {app !== '' && app !== remoteType ? (
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
