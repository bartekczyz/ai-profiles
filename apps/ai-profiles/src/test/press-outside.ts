import { act, fireEvent } from '@testing-library/react'

/**
 * Presses the mouse down on the page behind an open dialog. A dialog starts
 * listening for that a moment after it opens, so the first tick is let go by.
 */
export async function pressOutside() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0))
  })
  fireEvent.pointerDown(document.body)
}
