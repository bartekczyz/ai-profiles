import type { AppId } from '@/lib/types'

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { appSpecs } from '@/lib/app-registry'

import { DockIconConsentDialog } from './dock-icon-consent-dialog'

function setup(app: AppId = 'claude', onConfirm = vi.fn().mockResolvedValue(undefined)) {
  const onClose = vi.fn()
  render(<DockIconConsentDialog open app={app} onClose={onClose} onConfirm={onConfirm} />)
  return { onClose, onConfirm, user: userEvent.setup() }
}

/** The dialog with nothing to confirm: it only explains. */
function setupReading(app: AppId = 'claude') {
  const onClose = vi.fn()
  render(<DockIconConsentDialog open app={app} onClose={onClose} />)
  return { onClose, user: userEvent.setup() }
}

describe('DockIconConsentDialog', () => {
  it('confirms from the primary action', async () => {
    const { user, onConfirm, onClose } = setup()
    await user.click(screen.getByRole('button', { name: /Turn on/ }))
    expect(onConfirm).toHaveBeenCalledTimes(1)
    // Closing it is up to whoever handles the confirmation.
    expect(onClose).not.toHaveBeenCalled()
  })

  it('confirms with Enter, wherever focus is inside it', async () => {
    const { user, onConfirm } = setup()
    await user.keyboard('{Enter}')
    expect(onConfirm).toHaveBeenCalledTimes(1)
  })

  it('closes without confirming when cancelled', async () => {
    const { user, onConfirm, onClose } = setup()
    await user.click(screen.getByRole('button', { name: /^Cancel/ }))
    expect(onClose).toHaveBeenCalledTimes(1)
    expect(onConfirm).not.toHaveBeenCalled()
  })

  it('confirms once, however often it is pressed while the confirmation is being recorded', async () => {
    let finish: () => void = () => {}
    const onConfirm = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve
        }),
    )
    const { user } = setup('claude', onConfirm)

    await user.click(screen.getByRole('button', { name: /Turn on/ }))
    await user.keyboard('{Enter}')
    await user.click(screen.getByRole('button', { name: /Turn on/ }))

    expect(onConfirm).toHaveBeenCalledTimes(1)
    finish()
  })

  it.each([
    ['claude', /Cowork can't use folders/],
    ['codex', /Notifications don't work/],
  ] as const)('names what %s loses, among what turning it on changes', (app, loss) => {
    setup(app)

    const onColumn = 1
    expect(screen.getByText(loss).closest('td')?.cellIndex).toBe(onColumn)
  })

  it("names only the app's own loss", () => {
    setup('claude')
    expect(screen.queryByText(/Notifications don't work/)).toBeNull()
  })

  describe('when it has nothing to confirm', () => {
    it('asks nothing, and closes from its one button', async () => {
      const { user, onClose } = setupReading()
      expect(screen.queryByRole('button', { name: /Turn on/ })).toBeNull()
      await user.click(screen.getByRole('button', { name: /^Close/ }))
      expect(onClose).toHaveBeenCalledTimes(1)
    })

    it('closes on Enter, wherever focus is inside it', async () => {
      const { user, onClose } = setupReading()
      await user.keyboard('{Enter}')
      expect(onClose).toHaveBeenCalledTimes(1)
    })

    it('still names what an app loses', () => {
      const { cost } = appSpecs.codex.dockIcon
      setupReading('codex')
      expect(screen.getByText(cost as string)).toBeInTheDocument()
    })
  })
})
