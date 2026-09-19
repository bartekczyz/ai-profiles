import type { ComponentProps } from 'react'

import { act, fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { Dialog } from './dialog'

function setup(props: Partial<ComponentProps<typeof Dialog>> = {}) {
  const onClose = vi.fn()
  render(
    <Dialog open title="Title" description="Description" onClose={onClose} {...props}>
      <button type="button">Inside</button>
    </Dialog>,
  )
  return { onClose, user: userEvent.setup() }
}

/**
 * Presses the mouse down on the page behind the dialog. The dialog starts
 * listening for that a moment after it opens, so the first tick is let go by.
 */
async function pressOutside() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0))
  })
  fireEvent.pointerDown(document.body)
}

describe('Dialog', () => {
  it('closes when the page behind it is pressed', async () => {
    const { onClose } = setup()
    await pressOutside()
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('stays open when the page behind it is pressed, if it is not to close that way', async () => {
    const { onClose } = setup({ closeOnOutsideClick: false })
    await pressOutside()
    expect(onClose).not.toHaveBeenCalled()
  })

  it('still closes on Escape when it is not to close on a press outside', async () => {
    const { onClose, user } = setup({ closeOnOutsideClick: false })
    await user.keyboard('{Escape}')
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('does not close when something inside it is pressed', async () => {
    const { onClose, user } = setup()
    await user.click(screen.getByRole('button', { name: 'Inside' }))
    expect(onClose).not.toHaveBeenCalled()
  })
})
