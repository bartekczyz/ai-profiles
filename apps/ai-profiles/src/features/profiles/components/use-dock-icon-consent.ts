import { useState } from 'react'

import { useToast } from '@/design'
import { extractErrorMessage } from '@/lib/extract-error-message'

type Options = {
  /**
   * Whether the user has already confirmed they understand what a Dock icon of
   * its own involves.
   */
  acknowledged: boolean
  /**
   * Takes the setting's new value once it has been chosen, and confirmed if it
   * had to be.
   */
  onChoose: (next: boolean) => void
  /**
   * Records the acknowledgement, so the explanation is not shown again.
   */
  onAcknowledge: () => Promise<void>
}

type DockIconConsent = {
  /**
   * Whether the explanation is on screen, as a question or to be read.
   */
  open: boolean
  /**
   * Whether what is on screen is a question, to be answered by turning the
   * setting on or not, rather than an explanation to read.
   */
  asking: boolean
  /**
   * Turns the setting on or off. Turning it on before it has been acknowledged
   * opens the explanation instead, and the setting stays as it was until that is
   * confirmed.
   */
  choose: (next: boolean) => void
  /**
   * Records the acknowledgement and turns the setting on. If it cannot be
   * recorded the explanation stays up, and the setting stays off.
   */
  confirm: () => Promise<void>
  /**
   * Closes the explanation without turning the setting on.
   */
  cancel: () => void
  /**
   * Opens the explanation to read, whether or not it has been acknowledged. It
   * asks nothing and changes nothing.
   */
  explain: () => void
}

/**
 * Puts the Dock icon setting behind a one-time explanation. Turning it on builds
 * a re-signed copy of the app and costs a sign-in, which is worth saying at the
 * moment of choosing rather than after the fact; once it has been said, turning
 * it on is just a click.
 *
 * Only turning it on is gated. Turning it off gives nothing up.
 */
export function useDockIconConsent({ acknowledged, onChoose, onAcknowledge }: Options): DockIconConsent {
  const toast = useToast()
  const [shown, setShown] = useState<'question' | 'explanation' | null>(null)

  return {
    open: shown !== null,
    asking: shown === 'question',
    choose: (next) => {
      if (next && !acknowledged) {
        setShown('question')
        return
      }
      onChoose(next)
    },
    confirm: async () => {
      try {
        await onAcknowledge()
      } catch (caught) {
        toast.error('Could not save your choice.', extractErrorMessage(caught))
        return
      }
      onChoose(true)
      setShown(null)
    },
    cancel: () => setShown(null),
    explain: () => setShown('explanation'),
  }
}
