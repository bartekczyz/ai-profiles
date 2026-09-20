import type { Dependencies, Profile } from '@/lib/types'

import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'
import { pressOutside } from '@/test/press-outside'

import { EditProfileDialog } from './edit-profile-dialog'

type DialogProps = Parameters<typeof EditProfileDialog>[0]

/**
 * The Dock icon explanation is something most of these cases have nothing to
 * say about, so it defaults to not being acknowledged, with nothing to record it.
 */
type RenderProps = Omit<DialogProps, 'dockIconAcknowledged' | 'onAcknowledgeDockIcon'> &
  Partial<Pick<DialogProps, 'dockIconAcknowledged' | 'onAcknowledgeDockIcon'>>

function renderEdit(props: RenderProps) {
  return render(
    <ToastProvider>
      <EditProfileDialog
        dockIconAcknowledged={false}
        onAcknowledgeDockIcon={vi.fn().mockResolvedValue(undefined)}
        {...props}
      />
    </ToastProvider>,
  )
}

function fixture(overrides: Partial<Profile> = {}): Profile {
  return {
    id: '1',
    app: 'claude',
    name: 'Personal',
    slug: 'personal',
    color: '#d97757',
    createdAt: '2026-05-20T12:00:00Z',
    distinctDockIcon: false,
    lastUsedAt: null,

    surfaces: { gui: true, cli: true },
    ...overrides,
  }
}

const DEPS: Dependencies = {
  apps: {
    claude: { guiInstalled: true, cliInstalled: true },
    codex: { guiInstalled: false, cliInstalled: false },
  },
  localBinOnPath: true,
}

describe('EditProfileDialog', () => {
  it('disables Save when nothing has changed', () => {
    const onSave = vi.fn().mockResolvedValue(undefined)
    renderEdit({ open: true, profile: fixture(), dependencies: DEPS, onClose: vi.fn(), onSave })
    expect(screen.getByRole('button', { name: /^Save/ })).toBeDisabled()
  })

  it('enables Save when the name changes', async () => {
    const user = userEvent.setup()
    renderEdit({
      open: true,
      profile: fixture(),
      dependencies: DEPS,
      onClose: vi.fn(),
      onSave: vi.fn().mockResolvedValue(undefined),
    })
    const input = screen.getByLabelText('Name') as HTMLInputElement
    await user.clear(input)
    await user.type(input, 'Renamed')
    expect(screen.getByRole('button', { name: /^Save/ })).toBeEnabled()
  })

  it('resets form state when a different profile is opened', () => {
    const { rerender } = render(
      <ToastProvider>
        <EditProfileDialog
          open
          profile={fixture({ id: '1', name: 'Personal' })}
          dependencies={DEPS}
          dockIconAcknowledged={false}
          onAcknowledgeDockIcon={vi.fn()}
          onClose={vi.fn()}
          onSave={vi.fn().mockResolvedValue(undefined)}
        />
      </ToastProvider>,
    )

    rerender(
      <ToastProvider>
        <EditProfileDialog
          open
          profile={fixture({ id: '2', name: 'Work' })}
          dependencies={DEPS}
          dockIconAcknowledged={false}
          onAcknowledgeDockIcon={vi.fn()}
          onClose={vi.fn()}
          onSave={vi.fn().mockResolvedValue(undefined)}
        />
      </ToastProvider>,
    )

    expect(screen.getByLabelText('Name')).toHaveValue('Work')
  })

  it('calls onSave with the trimmed name and current surfaces', async () => {
    const user = userEvent.setup()
    const onSave = vi.fn().mockResolvedValue(undefined)
    const onClose = vi.fn()
    renderEdit({ open: true, profile: fixture(), dependencies: DEPS, onClose, onSave })
    const input = screen.getByLabelText('Name') as HTMLInputElement
    await user.clear(input)
    await user.type(input, '  Renamed  ')
    await user.click(screen.getByRole('button', { name: /^Save/ }))
    expect(onSave).toHaveBeenCalledWith({
      name: 'Renamed',
      color: '#d97757',
      surfaces: { gui: true, cli: true },
      distinctDockIcon: false,
    })
  })

  it('submits when the user presses Enter while focused on a surface checkbox', async () => {
    const user = userEvent.setup()
    const onSave = vi.fn().mockResolvedValue(undefined)
    renderEdit({ open: true, profile: fixture(), dependencies: DEPS, onClose: vi.fn(), onSave })
    const input = screen.getByLabelText('Name') as HTMLInputElement
    await user.clear(input)
    await user.type(input, 'Renamed')
    // After typing the new name, focus the desktop surface checkbox and hit
    // Enter. The dialog should submit without the checkbox toggling itself.
    const desktopCheckbox = screen.getByRole('checkbox', { name: /Desktop App launcher/ }) as HTMLButtonElement
    desktopCheckbox.focus()
    await user.keyboard('{Enter}')
    expect(onSave).toHaveBeenCalledWith({
      name: 'Renamed',
      color: '#d97757',
      surfaces: { gui: true, cli: true },
      distinctDockIcon: false,
    })
  })

  it('shows a toast (not an inline error) when onSave rejects, and keeps the dialog open', async () => {
    const user = userEvent.setup()
    const onSave = vi.fn().mockRejectedValue({ kind: 'Validation', message: 'name already in use' })
    const onClose = vi.fn()
    renderEdit({ open: true, profile: fixture(), dependencies: DEPS, onClose, onSave })
    const input = screen.getByLabelText('Name') as HTMLInputElement
    await user.clear(input)
    await user.type(input, 'Renamed')
    await user.click(screen.getByRole('button', { name: /^Save/ }))
    expect(await screen.findByText('Could not save profile.')).toBeInTheDocument()
    expect(screen.getAllByText(/name already in use/).length).toBeGreaterThan(0)
    expect(onClose).not.toHaveBeenCalled()
  })

  it('enables Save when a surface is toggled off', async () => {
    const user = userEvent.setup()
    const onSave = vi.fn().mockResolvedValue(undefined)
    renderEdit({ open: true, profile: fixture(), dependencies: DEPS, onClose: vi.fn(), onSave })
    // Surface toggle buttons live in the ProfileFormFields surface list
    await user.click(screen.getByRole('checkbox', { name: /Desktop App launcher/ }))
    expect(screen.getByRole('button', { name: /^Save/ })).toBeEnabled()
    await user.click(screen.getByRole('button', { name: /^Save/ }))
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ surfaces: { gui: false, cli: true } }))
  })
})

describe('EditProfileDialog — Dock icon', () => {
  // The explanation opens over the form, which hides it from the accessibility
  // tree; the option is still there to be read.
  function dockIconOption() {
    return screen.getByRole('checkbox', { name: /Distinct Dock icon/, hidden: true })
  }

  it('shows the profile’s current setting and offers to save a change to it', async () => {
    const user = userEvent.setup()
    const onSave = vi.fn().mockResolvedValue(undefined)
    renderEdit({
      open: true,
      profile: fixture({ distinctDockIcon: true }),
      dependencies: DEPS,
      dockIconAcknowledged: true,
      onClose: vi.fn(),
      onSave,
    })
    expect(dockIconOption()).toBeChecked()
    expect(screen.getByRole('button', { name: /^Save/ })).toBeDisabled()

    await user.click(dockIconOption())
    expect(screen.getByRole('button', { name: /^Save/ })).toBeEnabled()
    await user.click(screen.getByRole('button', { name: /^Save/ }))

    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ distinctDockIcon: false }))
  })

  it('saves it turned on without any explanation once that has been acknowledged', async () => {
    const user = userEvent.setup()
    const onSave = vi.fn().mockResolvedValue(undefined)
    const onAcknowledgeDockIcon = vi.fn().mockResolvedValue(undefined)
    renderEdit({
      open: true,
      profile: fixture(),
      dependencies: DEPS,
      dockIconAcknowledged: true,
      onClose: vi.fn(),
      onAcknowledgeDockIcon,
      onSave,
    })

    await user.click(dockIconOption())
    await user.click(screen.getByRole('button', { name: /^Save/ }))

    expect(screen.getAllByRole('dialog')).toHaveLength(1)
    expect(onAcknowledgeDockIcon).not.toHaveBeenCalled()
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ distinctDockIcon: true }))
  })

  it('explains itself the first time it is turned on, and saves it on once that is confirmed', async () => {
    const user = userEvent.setup()
    const onSave = vi.fn().mockResolvedValue(undefined)
    const onAcknowledgeDockIcon = vi.fn().mockResolvedValue(undefined)
    renderEdit({
      open: true,
      profile: fixture(),
      dependencies: DEPS,
      onClose: vi.fn(),
      onAcknowledgeDockIcon,
      onSave,
    })

    await user.click(dockIconOption())
    const explanation = await screen.findByRole('dialog')
    expect(dockIconOption()).not.toBeChecked()
    expect(onAcknowledgeDockIcon).not.toHaveBeenCalled()

    await user.click(within(explanation).getByRole('button', { name: /Turn on/ }))
    await waitFor(() => {
      expect(dockIconOption()).toBeChecked()
    })
    expect(onAcknowledgeDockIcon).toHaveBeenCalledTimes(1)
    await user.click(screen.getByRole('button', { name: /^Save/ }))

    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ distinctDockIcon: true }))
  })

  it('changes nothing when the explanation is dismissed', async () => {
    const user = userEvent.setup()
    const onAcknowledgeDockIcon = vi.fn().mockResolvedValue(undefined)
    renderEdit({
      open: true,
      profile: fixture(),
      dependencies: DEPS,
      onClose: vi.fn(),
      onAcknowledgeDockIcon,
      onSave: vi.fn().mockResolvedValue(undefined),
    })

    await user.click(dockIconOption())
    const explanation = await screen.findByRole('dialog')
    await user.click(within(explanation).getByRole('button', { name: /^Cancel/ }))

    await waitFor(() => {
      expect(explanation).not.toBeInTheDocument()
    })
    expect(onAcknowledgeDockIcon).not.toHaveBeenCalled()
    expect(dockIconOption()).not.toBeChecked()
    expect(screen.getByRole('button', { name: /^Save/ })).toBeDisabled()
  })

  it('never asks before turning it off', async () => {
    const user = userEvent.setup()
    const onSave = vi.fn().mockResolvedValue(undefined)
    renderEdit({
      open: true,
      profile: fixture({ distinctDockIcon: true }),
      dependencies: DEPS,
      dockIconAcknowledged: false,
      onClose: vi.fn(),
      onSave,
    })

    await user.click(dockIconOption())

    expect(screen.getAllByRole('dialog')).toHaveLength(1)
    expect(dockIconOption()).not.toBeChecked()
  })

  it('starts from the setting of whichever profile is opened', () => {
    const { rerender } = render(
      <ToastProvider>
        <EditProfileDialog
          open
          profile={fixture({ id: '1', distinctDockIcon: false })}
          dependencies={DEPS}
          dockIconAcknowledged
          onClose={vi.fn()}
          onAcknowledgeDockIcon={vi.fn()}
          onSave={vi.fn().mockResolvedValue(undefined)}
        />
      </ToastProvider>,
    )
    expect(dockIconOption()).not.toBeChecked()

    rerender(
      <ToastProvider>
        <EditProfileDialog
          open
          profile={fixture({ id: '2', distinctDockIcon: true })}
          dependencies={DEPS}
          dockIconAcknowledged
          onClose={vi.fn()}
          onAcknowledgeDockIcon={vi.fn()}
          onSave={vi.fn().mockResolvedValue(undefined)}
        />
      </ToastProvider>,
    )

    expect(dockIconOption()).toBeChecked()
  })

  it('is unavailable while the desktop launcher is off', async () => {
    const user = userEvent.setup()
    renderEdit({
      open: true,
      profile: fixture({ distinctDockIcon: true }),
      dependencies: DEPS,
      dockIconAcknowledged: true,
      onClose: vi.fn(),
      onSave: vi.fn().mockResolvedValue(undefined),
    })

    await user.click(screen.getByRole('checkbox', { name: /Desktop App launcher/ }))

    expect(dockIconOption()).toBeDisabled()
    expect(dockIconOption()).not.toBeChecked()
  })
})

describe('EditProfileDialog — explaining the Dock icon', () => {
  function infoButton() {
    return screen.getByRole('button', { name: /About the Dock icon/ })
  }

  function open(overrides: Partial<Profile> = {}, dockIconAcknowledged = true) {
    renderEdit({
      open: true,
      profile: fixture(overrides),
      dependencies: DEPS,
      dockIconAcknowledged,
      onClose: vi.fn(),
      onSave: vi.fn().mockResolvedValue(undefined),
    })
    return userEvent.setup()
  }

  it('can be read at any time, not only the first time the setting is turned on', async () => {
    const user = open()
    await user.click(infoButton())
    expect(await screen.findByRole('dialog', { name: /A Dock icon of its own/ })).toBeInTheDocument()
  })

  it('asks nothing when it is only being read, and leaves the setting as it was', async () => {
    const user = open()
    await user.click(infoButton())

    expect(screen.queryByRole('button', { name: /Turn on/ })).toBeNull()
    await user.click(screen.getByRole('button', { name: /^Close/ }))

    expect(screen.queryByRole('dialog', { name: /A Dock icon of its own/ })).toBeNull()
    expect(screen.getByRole('checkbox', { name: /Distinct Dock icon/ })).not.toBeChecked()
  })

  it('can be read with the desktop launcher off, when the setting itself cannot be changed', async () => {
    const user = open({ distinctDockIcon: true })
    await user.click(screen.getByRole('checkbox', { name: /Desktop App launcher/ }))
    expect(screen.getByRole('checkbox', { name: /Distinct Dock icon/ })).toBeDisabled()

    await user.click(infoButton())

    expect(await screen.findByRole('dialog', { name: /A Dock icon of its own/ })).toBeInTheDocument()
  })

  it('is still a question, not just an explanation, when the setting is turned on for the first time', async () => {
    const user = open({}, false)
    await user.click(screen.getByRole('checkbox', { name: /Distinct Dock icon/ }))
    expect(await screen.findByRole('button', { name: /Turn on/ })).toBeInTheDocument()
  })
})

describe('EditProfileDialog — leaving it', () => {
  function open() {
    const onClose = vi.fn()
    renderEdit({
      open: true,
      profile: fixture(),
      dependencies: DEPS,
      onClose,
      onSave: vi.fn().mockResolvedValue(undefined),
    })
    return { onClose, user: userEvent.setup() }
  }

  it('is not closed by a press on the page behind it, which would lose what was chosen in it', async () => {
    const { onClose } = open()
    await pressOutside()
    expect(onClose).not.toHaveBeenCalled()
  })

  it('closes from Cancel', async () => {
    const { onClose, user } = open()
    await user.click(screen.getByRole('button', { name: /^Cancel/ }))
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('closes on Escape, which Cancel is labelled with', async () => {
    const { onClose, user } = open()
    await user.keyboard('{Escape}')
    expect(onClose).toHaveBeenCalledTimes(1)
  })
})
