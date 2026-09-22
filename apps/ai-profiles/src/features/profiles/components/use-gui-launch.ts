import { useRef, useState } from 'react'

export type GuiLaunch = {
  /**
   * Whether a launch is under way. Drives the desktop row's control, which
   * says "Opening" and takes no press while it is true.
   */
  opening: boolean
  /**
   * Run one launch, unless one is already running. Rejects as `action` does,
   * so the caller still reports the failure.
   */
  run: (action: () => Promise<unknown>) => Promise<void>
}

/**
 * Holds a desktop launch while it happens.
 *
 * The commands behind a launch return once the app's process is actually
 * there, which is seconds for a cold start and longer for a profile whose own
 * bundle has to be rebuilt first, so the control has something worth reporting.
 *
 * **Call this above the detail pane's `Suspense` boundary.** The boundary's
 * fallback and its child are separate subtrees that both render the surfaces
 * panel, so the panel is unmounted and replaced the moment the profile's paths
 * land. State kept inside it would be dropped mid-launch, taking the label and
 * the guard below with it — and the pane deliberately keeps the control live
 * during exactly that window, so an early click is the case this has to
 * survive.
 */
export function useGuiLaunch(): GuiLaunch {
  const [opening, setOpening] = useState(false)
  const inFlight = useRef(false)

  return {
    opening,
    run: async (action) => {
      // A ref rather than the state above: two presses in one tick would both
      // read the old state, and the keyboard route can be holding a callback
      // from an earlier render.
      if (inFlight.current) {
        return
      }
      inFlight.current = true
      setOpening(true)
      try {
        await action()
      } finally {
        inFlight.current = false
        setOpening(false)
      }
    },
  }
}
