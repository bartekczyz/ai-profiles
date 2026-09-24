import type { Dependencies, RemoteHost } from '@/lib/types'

import { useState } from 'react'

import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'
import { appSpecs } from '@/lib/app-registry'
import { pressOutside } from '@/test/press-outside'
import { renderWithQuery } from '@/test/render-with-query'

import { CreateProfileDialog } from './create-profile-dialog'

vi.mock('@/lib/commands', async () => {
  const actual = await vi.importActual<typeof import('@/lib/commands')>('@/lib/commands')
  // The server a remote profile is made on already has `taken`.
  return { ...actual, remoteListAccounts: vi.fn(async () => [{ name: 'taken' }]) }
})

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

const onlyCodexInstalled: Dependencies = {
  apps: {
    claude: { guiInstalled: false, cliInstalled: false },
    codex: { guiInstalled: true, cliInstalled: true },
  },
  localBinOnPath: true,
}

function setup(overrides: Partial<Parameters<typeof CreateProfileDialog>[0]> = {}) {
  const onClose = vi.fn()
  const onCreate = vi.fn().mockResolvedValue(undefined)
  const onAcknowledgeDockIcon = vi.fn().mockResolvedValue(undefined)
  renderWithQuery(
    <ToastProvider>
      <CreateProfileDialog
        open
        dependencies={ONLY_CLAUDE_INSTALLED}
        dockIconAcknowledged={false}
        onClose={onClose}
        onAcknowledgeDockIcon={onAcknowledgeDockIcon}
        onCreate={onCreate}
        {...overrides}
      />
    </ToastProvider>,
  )
  return { onClose, onCreate, onAcknowledgeDockIcon, user: userEvent.setup() }
}

const server: RemoteHost = {
  id: 'h1',
  label: 'xjopa1',
  hostname: 'xjopa1',
  addresses: [],
  fingerprint: 'ab'.repeat(32),
  clientId: 'c1',
  pairedAt: '2026-09-22T00:00:00Z',
  lastGoodAddress: null,
  profiles: {},
}

describe('CreateProfileDialog — Claude CLI Remote', () => {
  it('makes a profile on the chosen server, with a name the server can use', async () => {
    const onCreateRemote = vi.fn().mockResolvedValue(undefined)
    const { user, onCreate, onClose } = setup({
      remoteHosts: [server],
      initialRemoteHostId: 'h1',
      onCreateRemote,
    })
    expect(screen.getByLabelText('Server')).toHaveTextContent('xjopa1')
    expect(screen.queryByText('Surfaces')).toBeNull()
    await user.type(screen.getByLabelText('Name'), '../x')
    expect(screen.getByRole('button', { name: /Create profile/ })).toBeDisabled()
    await user.clear(screen.getByLabelText('Name'))
    await user.type(screen.getByLabelText('Name'), 'Taken')
    expect(await screen.findByText('xjopa1 already has a profile called Taken.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Create profile/ })).toBeDisabled()
    await user.clear(screen.getByLabelText('Name'))
    await user.type(screen.getByLabelText('Name'), 'work')
    expect(screen.getByText('On xjopa1: ~/.claude-accounts/work')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /Create profile/ }))
    await waitFor(() =>
      expect(onCreateRemote).toHaveBeenCalledWith({ hostId: 'h1', name: 'work', color: expect.any(String) }),
    )
    expect(onCreate).not.toHaveBeenCalled()
    expect(onClose).toHaveBeenCalled()
  })
})

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
      distinctDockIcon: false,
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
      distinctDockIcon: false,
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
      distinctDockIcon: false,
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
    renderWithQuery(
      <ToastProvider>
        <CreateProfileDialog
          open
          dependencies={ONLY_CLAUDE_INSTALLED}
          dockIconAcknowledged={false}
          onClose={onClose}
          onAcknowledgeDockIcon={vi.fn()}
          onCreate={onCreate}
        />
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
    renderWithQuery(
      <ToastProvider>
        <CreateProfileDialog
          open
          dependencies={deps}
          dockIconAcknowledged={false}
          onClose={vi.fn()}
          onAcknowledgeDockIcon={vi.fn()}
          onCreate={onCreate}
        />
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
    renderWithQuery(
      <ToastProvider>
        <CreateProfileDialog
          open
          dockIconAcknowledged={false}
          onAcknowledgeDockIcon={vi.fn()}
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
      distinctDockIcon: false,
    })
  })
})

describe('CreateProfileDialog — app-type picker behaviour', () => {
  it('pre-selects codex and calls onCreate with app: codex when only Codex is installed', async () => {
    const onCreate = vi.fn().mockResolvedValue(undefined)
    renderWithQuery(
      <ToastProvider>
        <CreateProfileDialog
          open
          dockIconAcknowledged={false}
          onAcknowledgeDockIcon={vi.fn()}
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
    renderWithQuery(
      <ToastProvider>
        <CreateProfileDialog
          open
          dependencies={BOTH_INSTALLED}
          dockIconAcknowledged={false}
          onClose={vi.fn()}
          onAcknowledgeDockIcon={vi.fn()}
          onCreate={onCreate}
        />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Name'), 'Work')
    // Submit attempt with no app selected — onCreate must not be called
    await user.click(screen.getByRole('button', { name: /^Create profile/ }))
    expect(onCreate).not.toHaveBeenCalled()
  })

  it('renders the Codex GUI install link when Codex is selected and GUI is missing', async () => {
    renderWithQuery(
      <ToastProvider>
        <CreateProfileDialog
          open
          dockIconAcknowledged={false}
          onAcknowledgeDockIcon={vi.fn()}
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
    const link = screen.getByRole('link', { name: /ChatGPT Desktop/ })
    expect(link).toHaveAttribute('href', appSpecs.codex.gui.installUrl)
  })
})

describe('CreateProfileDialog — Dock icon', () => {
  // The explanation opens over the form, which hides it from the accessibility
  // tree; the option is still there to be read.
  function dockIconOption() {
    return screen.getByRole('checkbox', { name: /Distinct Dock icon/, hidden: true })
  }

  async function create(user: ReturnType<typeof userEvent.setup>) {
    await user.type(screen.getByLabelText('Name'), 'Personal')
    await user.click(screen.getByRole('button', { name: /^Create profile/ }))
  }

  it('starts off until the user has acknowledged what it involves', async () => {
    const { user, onCreate } = setup()
    expect(dockIconOption()).not.toBeChecked()
    await create(user)
    expect(onCreate).toHaveBeenCalledWith(expect.objectContaining({ distinctDockIcon: false }))
  })

  it('starts on, once acknowledged, for an app where it costs nothing they would notice', async () => {
    const { user, onCreate } = setup({ dockIconAcknowledged: true })
    expect(dockIconOption()).toBeChecked()
    await create(user)
    expect(onCreate).toHaveBeenCalledWith(expect.objectContaining({ distinctDockIcon: true }))
  })

  it('starts off, even once acknowledged, for an app where it has a cost', async () => {
    const { user, onCreate } = setup({ dependencies: onlyCodexInstalled, dockIconAcknowledged: true })
    expect(dockIconOption()).not.toBeChecked()
    await create(user)
    expect(onCreate).toHaveBeenCalledWith(expect.objectContaining({ app: 'codex', distinctDockIcon: false }))
  })

  it('explains itself before turning on the first time, and only then records that and turns on', async () => {
    const { user, onCreate, onAcknowledgeDockIcon } = setup()
    await user.click(dockIconOption())

    const explanation = await screen.findByRole('dialog')
    expect(onAcknowledgeDockIcon).not.toHaveBeenCalled()
    expect(dockIconOption()).not.toBeChecked()

    await user.click(within(explanation).getByRole('button', { name: /Turn on/ }))
    await waitFor(() => {
      expect(dockIconOption()).toBeChecked()
    })
    expect(onAcknowledgeDockIcon).toHaveBeenCalledTimes(1)

    await create(user)
    expect(onCreate).toHaveBeenCalledWith(expect.objectContaining({ distinctDockIcon: true }))
  })

  it('leaves it off, and records nothing, when the explanation is dismissed', async () => {
    const { user, onAcknowledgeDockIcon } = setup()
    await user.click(dockIconOption())
    const explanation = await screen.findByRole('dialog')

    await user.click(within(explanation).getByRole('button', { name: /^Cancel/ }))

    await waitFor(() => {
      expect(explanation).not.toBeInTheDocument()
    })
    expect(onAcknowledgeDockIcon).not.toHaveBeenCalled()
    expect(dockIconOption()).not.toBeChecked()
  })

  it('does not explain itself again once acknowledged', async () => {
    const { user, onAcknowledgeDockIcon } = setup({ dockIconAcknowledged: true })
    await user.click(dockIconOption())
    expect(dockIconOption()).not.toBeChecked()

    await user.click(dockIconOption())

    expect(dockIconOption()).toBeChecked()
    expect(screen.getAllByRole('dialog')).toHaveLength(1)
    expect(onAcknowledgeDockIcon).not.toHaveBeenCalled()
  })

  it('explains itself exactly once, not again the next time it is turned on', async () => {
    function Harness() {
      const [acknowledged, setAcknowledged] = useState(false)
      return (
        <ToastProvider>
          <CreateProfileDialog
            open
            dependencies={ONLY_CLAUDE_INSTALLED}
            dockIconAcknowledged={acknowledged}
            onClose={vi.fn()}
            onAcknowledgeDockIcon={async () => setAcknowledged(true)}
            onCreate={vi.fn().mockResolvedValue(undefined)}
          />
        </ToastProvider>
      )
    }
    renderWithQuery(<Harness />)
    const user = userEvent.setup()
    await user.click(dockIconOption())
    const explanation = await screen.findByRole('dialog')
    await user.click(within(explanation).getByRole('button', { name: /Turn on/ }))
    await waitFor(() => {
      expect(dockIconOption()).toBeChecked()
    })

    await user.click(dockIconOption())
    expect(dockIconOption()).not.toBeChecked()
    await user.click(dockIconOption())

    expect(dockIconOption()).toBeChecked()
    expect(screen.getAllByRole('dialog')).toHaveLength(1)
  })

  it('keeps it off, and says why, when the acknowledgement cannot be saved', async () => {
    const { user } = setup({ onAcknowledgeDockIcon: vi.fn().mockRejectedValue(new Error('disk is full')) })
    await user.click(dockIconOption())
    const explanation = await screen.findByRole('dialog')

    await user.click(within(explanation).getByRole('button', { name: /Turn on/ }))

    // The toast text is also mirrored into a live region for screen readers.
    expect((await screen.findAllByText(/disk is full/)).length).toBeGreaterThan(0)
    expect(explanation).toBeInTheDocument()
    expect(dockIconOption()).not.toBeChecked()
  })

  it('is unavailable, and not sent, while the desktop launcher is off', async () => {
    const { user, onCreate } = setup({ dockIconAcknowledged: true })
    await user.click(screen.getByRole('checkbox', { name: /Desktop App launcher/ }))

    expect(dockIconOption()).toBeDisabled()
    expect(dockIconOption()).not.toBeChecked()
    await create(user)
    expect(onCreate).toHaveBeenCalledWith(
      expect.objectContaining({ surfaces: { gui: false, cli: true }, distinctDockIcon: false }),
    )
  })

  it('puts what the app loses next to the option, before the user turns it on', () => {
    const { cost } = appSpecs.codex.dockIcon
    expect(cost).not.toBeNull()

    setup({ dependencies: onlyCodexInstalled })

    expect(dockIconOption()).toHaveTextContent(cost as string)
  })

  it('says nothing of a loss for an app that has none', () => {
    setup()
    expect(dockIconOption()).not.toHaveTextContent(appSpecs.codex.dockIcon.cost as string)
  })
})

describe('CreateProfileDialog — explaining the Dock icon', () => {
  it('can be read at any time, not only the first time the setting is turned on', async () => {
    const { user } = setup({ dockIconAcknowledged: true })
    await user.click(screen.getByRole('button', { name: /About the Dock icon/ }))
    expect(await screen.findByRole('dialog', { name: /A Dock icon of its own/ })).toBeInTheDocument()
  })

  it('asks nothing when it is only being read, and leaves the setting as it was', async () => {
    const { user, onAcknowledgeDockIcon } = setup()
    const before = screen.getByRole('checkbox', { name: /Distinct Dock icon/ }).getAttribute('aria-checked')

    await user.click(screen.getByRole('button', { name: /About the Dock icon/ }))
    expect(screen.queryByRole('button', { name: /Turn on/ })).toBeNull()
    await user.click(screen.getByRole('button', { name: /^Close/ }))

    expect(screen.getByRole('checkbox', { name: /Distinct Dock icon/ }).getAttribute('aria-checked')).toBe(before)
    expect(onAcknowledgeDockIcon).not.toHaveBeenCalled()
  })
})

describe('CreateProfileDialog — leaving it', () => {
  it('is not closed by a press on the page behind it, which would lose what was filled in', async () => {
    const { onClose } = setup()
    await pressOutside()
    expect(onClose).not.toHaveBeenCalled()
  })

  it('closes from Cancel', async () => {
    const { onClose, user } = setup()
    await user.click(screen.getByRole('button', { name: /^Cancel/ }))
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('closes on Escape, which Cancel is labelled with', async () => {
    const { onClose, user } = setup()
    await user.keyboard('{Escape}')
    expect(onClose).toHaveBeenCalledTimes(1)
  })
})
