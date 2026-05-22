import type { Dependencies } from '@/lib/types'

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'

import { CreateProfileDialog } from './create-profile-dialog'

const ALL_INSTALLED: Dependencies = {
  claudeAppInstalled: true,
  claudeCliInstalled: true,
  localBinOnPath: true,
}

function setup(overrides: Partial<Parameters<typeof CreateProfileDialog>[0]> = {}) {
  const onClose = vi.fn()
  const onCreate = vi.fn().mockResolvedValue(undefined)
  render(
    <ToastProvider>
      <CreateProfileDialog open dependencies={ALL_INSTALLED} onClose={onClose} onCreate={onCreate} {...overrides} />
    </ToastProvider>,
  )
  return { onClose, onCreate, user: userEvent.setup() }
}

describe('CreateProfileDialog', () => {
  it('disables Create when name is empty', () => {
    setup()
    expect(screen.getByRole('button', { name: /^Create profile/ })).toBeDisabled()
  })

  it('disables Create when no surfaces are selected', async () => {
    const { user } = setup()
    await user.type(screen.getByLabelText('Name'), 'Personal')
    await user.click(screen.getByRole('checkbox', { name: /Desktop App launcher/ }))
    await user.click(screen.getByRole('checkbox', { name: /Claude Code CLI wrapper/ }))
    expect(screen.getByRole('button', { name: /^Create profile/ })).toBeDisabled()
  })

  it('enables Create with a valid name + at least one surface', async () => {
    const { user } = setup()
    await user.type(screen.getByLabelText('Name'), 'Personal')
    expect(screen.getByRole('button', { name: /^Create profile/ })).toBeEnabled()
  })

  it('shows a live slug preview as the user types', async () => {
    const { user } = setup()
    await user.type(screen.getByLabelText('Name'), 'Acme Work')
    expect(screen.getByText('Slug: acme-work')).toBeInTheDocument()
  })

  it('calls onCreate with trimmed name on submit', async () => {
    const { user, onCreate, onClose } = setup()
    await user.type(screen.getByLabelText('Name'), '  Personal  ')
    await user.click(screen.getByRole('button', { name: /^Create profile/ }))
    expect(onCreate).toHaveBeenCalledWith({
      name: 'Personal',
      color: '#d97757',
      surfaces: { gui: true, cli: true },
    })
    expect(onClose).toHaveBeenCalled()
  })

  it('submits when the user presses Enter from inside the name input', async () => {
    const { user, onCreate } = setup()
    const nameInput = screen.getByLabelText('Name') as HTMLInputElement
    await user.type(nameInput, 'Personal{Enter}')
    expect(onCreate).toHaveBeenCalledWith({
      name: 'Personal',
      color: '#d97757',
      surfaces: { gui: true, cli: true },
    })
  })

  it('submits when the user presses Enter while focused on a surface checkbox', async () => {
    const { user, onCreate } = setup()
    await user.type(screen.getByLabelText('Name'), 'Personal')
    // Tab through the dialog until the first SurfaceToggle button has focus.
    // (Name input → color swatches → hex input → desktop checkbox.)
    const desktopCheckbox = screen.getByRole('checkbox', { name: /Desktop App launcher/ }) as HTMLButtonElement
    desktopCheckbox.focus()
    await user.keyboard('{Enter}')
    expect(onCreate).toHaveBeenCalledWith({
      name: 'Personal',
      color: '#d97757',
      // Both surfaces still selected — preventDefault on the dialog-level
      // Enter handler stops the checkbox from toggling itself off.
      surfaces: { gui: true, cli: true },
    })
  })

  it('does not submit when Enter is pressed and the form is invalid', async () => {
    const { user, onCreate } = setup()
    await user.type(screen.getByLabelText('Name'), '   {Enter}')
    expect(onCreate).not.toHaveBeenCalled()
  })

  it('shows a toast (not an inline message) when the backend rejects, and keeps the dialog open', async () => {
    const onCreate = vi.fn().mockRejectedValue({ kind: 'Validation', message: 'validation error: slug already exists' })
    const onClose = vi.fn()
    render(
      <ToastProvider>
        <CreateProfileDialog open dependencies={ALL_INSTALLED} onClose={onClose} onCreate={onCreate} />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Name'), 'Personal')
    await user.click(screen.getByRole('button', { name: /^Create profile/ }))
    expect(await screen.findByText('Could not create profile.')).toBeInTheDocument()
    expect(screen.getAllByText(/slug already exists/).length).toBeGreaterThan(0)
    expect(onClose).not.toHaveBeenCalled()
  })
})

describe('CreateProfileDialog — dependency awareness', () => {
  function renderWith(deps: Dependencies) {
    const onCreate = vi.fn().mockResolvedValue(undefined)
    render(
      <ToastProvider>
        <CreateProfileDialog open dependencies={deps} onClose={vi.fn()} onCreate={onCreate} />
      </ToastProvider>,
    )
    return { onCreate, user: userEvent.setup() }
  }

  it('disables the Desktop surface when Claude.app is missing', () => {
    renderWith({ ...ALL_INSTALLED, claudeAppInstalled: false })
    expect(screen.getByRole('checkbox', { name: /Desktop App launcher/ })).toBeDisabled()
    expect(screen.getByText(/Claude Desktop/)).toBeInTheDocument()
  })

  it('disables the CLI surface when claude is missing and shows the install hint', () => {
    renderWith({ ...ALL_INSTALLED, claudeCliInstalled: false })
    expect(screen.getByRole('checkbox', { name: /Claude Code CLI wrapper/ })).toBeDisabled()
    expect(screen.getByText(/npm install -g @anthropic-ai\/claude-code/)).toBeInTheDocument()
  })

  it('disables submit when both surfaces are unavailable', async () => {
    const { user } = renderWith({
      claudeAppInstalled: false,
      claudeCliInstalled: false,
      localBinOnPath: true,
    })
    await user.type(screen.getByLabelText('Name'), 'Personal')
    expect(screen.getByRole('button', { name: /^Create profile/ })).toBeDisabled()
  })

  it('submits only the available surface when one is missing', async () => {
    const onCreate = vi.fn().mockResolvedValue(undefined)
    render(
      <ToastProvider>
        <CreateProfileDialog
          open
          dependencies={{ ...ALL_INSTALLED, claudeAppInstalled: false }}
          onClose={vi.fn()}
          onCreate={onCreate}
        />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Name'), 'Personal')
    await user.click(screen.getByRole('button', { name: /^Create profile/ }))
    expect(onCreate).toHaveBeenCalledWith({
      name: 'Personal',
      color: '#d97757',
      surfaces: { gui: false, cli: true },
    })
  })
})
