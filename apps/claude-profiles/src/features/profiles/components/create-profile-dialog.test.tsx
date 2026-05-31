import type { Dependencies } from '@/lib/types'

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'
import { appSpecs } from '@/lib/app-registry'

import { CreateProfileDialog } from './create-profile-dialog'

const ONLY_CLAUDE_INSTALLED: Dependencies = {
  apps: {
    claude: { guiInstalled: true, cliInstalled: true },
    codex: { guiInstalled: false, cliInstalled: false },
  },
  localBinOnPath: true,
}

const BOTH_INSTALLED: Dependencies = {
  apps: {
    claude: { guiInstalled: true, cliInstalled: true },
    codex: { guiInstalled: true, cliInstalled: true },
  },
  localBinOnPath: true,
}

function setup(overrides: Partial<Parameters<typeof CreateProfileDialog>[0]> = {}) {
  const onClose = vi.fn()
  const onCreate = vi.fn().mockResolvedValue(undefined)
  render(
    <ToastProvider>
      <CreateProfileDialog
        open
        dependencies={ONLY_CLAUDE_INSTALLED}
        onClose={onClose}
        onCreate={onCreate}
        {...overrides}
      />
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

  it('calls onCreate with trimmed name and pre-selected app on submit', async () => {
    const { user, onCreate, onClose } = setup()
    await user.type(screen.getByLabelText('Name'), '  Personal  ')
    await user.click(screen.getByRole('button', { name: /^Create profile/ }))
    expect(onCreate).toHaveBeenCalledWith({
      app: 'claude',
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
      app: 'claude',
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
      app: 'claude',
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
        <CreateProfileDialog open dependencies={ONLY_CLAUDE_INSTALLED} onClose={onClose} onCreate={onCreate} />
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
    renderWith({
      apps: {
        claude: { guiInstalled: false, cliInstalled: true },
        codex: { guiInstalled: false, cliInstalled: false },
      },
      localBinOnPath: true,
    })
    expect(screen.getByRole('checkbox', { name: /Desktop App launcher/ })).toBeDisabled()
  })

  it('disables the CLI surface when claude CLI is missing', () => {
    renderWith({
      apps: {
        claude: { guiInstalled: true, cliInstalled: false },
        codex: { guiInstalled: false, cliInstalled: false },
      },
      localBinOnPath: true,
    })
    expect(screen.getByRole('checkbox', { name: /Claude Code CLI wrapper/ })).toBeDisabled()
  })

  it('disables submit when both surfaces are unavailable', async () => {
    const { user } = renderWith({
      apps: {
        claude: { guiInstalled: false, cliInstalled: false },
        codex: { guiInstalled: false, cliInstalled: false },
      },
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
          dependencies={{
            apps: {
              claude: { guiInstalled: false, cliInstalled: true },
              codex: { guiInstalled: false, cliInstalled: false },
            },
            localBinOnPath: true,
          }}
          onClose={vi.fn()}
          onCreate={onCreate}
        />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Name'), 'Personal')
    await user.click(screen.getByRole('button', { name: /^Create profile/ }))
    expect(onCreate).toHaveBeenCalledWith({
      app: 'claude',
      name: 'Personal',
      color: '#d97757',
      surfaces: { gui: false, cli: true },
    })
  })
})

describe('CreateProfileDialog — app-type picker behaviour', () => {
  it('pre-selects codex and calls onCreate with app: codex when only Codex is installed', async () => {
    const onCreate = vi.fn().mockResolvedValue(undefined)
    render(
      <ToastProvider>
        <CreateProfileDialog
          open
          dependencies={{
            apps: {
              claude: { guiInstalled: false, cliInstalled: false },
              codex: { guiInstalled: true, cliInstalled: false },
            },
            localBinOnPath: true,
          }}
          onClose={vi.fn()}
          onCreate={onCreate}
        />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Name'), 'Work')
    await user.click(screen.getByRole('button', { name: /^Create profile/ }))
    expect(onCreate).toHaveBeenCalledWith(expect.objectContaining({ app: 'codex' }))
  })

  it('blocks submit with both apps installed until the user picks one', async () => {
    const onCreate = vi.fn().mockResolvedValue(undefined)
    render(
      <ToastProvider>
        <CreateProfileDialog open dependencies={BOTH_INSTALLED} onClose={vi.fn()} onCreate={onCreate} />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Name'), 'Work')
    // Submit attempt with no app selected — onCreate must not be called
    await user.click(screen.getByRole('button', { name: /^Create profile/ }))
    expect(onCreate).not.toHaveBeenCalled()
  })

  it('renders the Codex GUI install link when Codex is selected and GUI is missing', async () => {
    render(
      <ToastProvider>
        <CreateProfileDialog
          open
          dependencies={{
            apps: {
              claude: { guiInstalled: false, cliInstalled: false },
              codex: { guiInstalled: false, cliInstalled: true },
            },
            localBinOnPath: true,
          }}
          onClose={vi.fn()}
          onCreate={vi.fn()}
        />
      </ToastProvider>,
    )
    // Codex is the only installed app — it is pre-selected
    const link = screen.getByRole('link', { name: /Codex Desktop/ })
    expect(link).toHaveAttribute('href', appSpecs.codex.gui.installUrl)
  })
})
