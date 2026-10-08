import type { Dependencies, Profile } from '@/lib/types'

import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'
import { checkCustomIcon, generatedIconPreview } from '@/lib/commands'
import { pressOutside } from '@/test/press-outside'
import { renderWithQuery } from '@/test/render-with-query'

import { EditProfileDialog } from './edit-profile-dialog'

// The generated icon the launcher-icon preview shows: the bytes of 'gen'.
vi.mock('@/lib/commands', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/commands')>()),
  generatedIconPreview: vi.fn(async () => new Uint8Array([0x67, 0x65, 0x6e]).buffer),
  checkCustomIcon: vi.fn(async () => {}),
}))

type DialogProps = Parameters<typeof EditProfileDialog>[0]

/**
 * The Dock icon explanation is something most of these cases have nothing to
 * say about, so it defaults to not being acknowledged, with nothing to record it.
 */
type RenderProps = Omit<DialogProps, 'dockIconAcknowledged' | 'onAcknowledgeDockIcon'> &
  Partial<Pick<DialogProps, 'dockIconAcknowledged' | 'onAcknowledgeDockIcon'>>

function renderEdit(props: RenderProps) {
  return renderWithQuery(
    <ToastProvider>
      <EditProfileDialog
        dockIconAcknowledged={false}
        customIcon={null}
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
    const { rerender } = renderWithQuery(
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
      iconChange: null,
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
      iconChange: null,
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
    const { rerender } = renderWithQuery(
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

describe('EditProfileDialog — launcher icon', () => {
  const png = new File([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], 'logo.png', { type: 'image/png' })
  const generated = 'data:image/png;base64,Z2Vu'
  const picked = 'data:image/png;base64,iVBORw=='
  const own = 'data:image/png;base64,b3du'

  function renderIcon(props: Partial<RenderProps> = {}) {
    const onSave = vi.fn().mockResolvedValue(undefined)
    renderEdit({ open: true, profile: fixture(), dependencies: DEPS, onClose: vi.fn(), onSave, ...props })
    return { user: userEvent.setup(), onSave }
  }

  async function previewShows(src: string) {
    await waitFor(() => {
      expect(screen.getByRole('img', { name: 'Launcher icon preview' })).toHaveAttribute('src', src)
    })
  }

  it('previews the generated icon in the profile’s color, with nothing to remove', async () => {
    renderIcon()
    await previewShows(generated)
    expect(generatedIconPreview).toHaveBeenCalledWith('claude', '#d97757')
    expect(screen.getByRole('button', { name: 'Choose custom icon' })).toBeEnabled()
    expect(screen.queryByRole('button', { name: 'Reset to default' })).toBeNull()
  })

  it('previews the generated icon in a color typed but not saved yet, and only once it is one', async () => {
    const { user } = renderIcon()
    await previewShows(generated)
    const hex = screen.getByLabelText('Custom hex color')
    await user.clear(hex)
    await user.type(hex, '#123456')
    await waitFor(() => {
      expect(generatedIconPreview).toHaveBeenLastCalledWith('claude', '#123456')
    })
    const colors = vi.mocked(generatedIconPreview).mock.calls.map(([, color]) => color)
    expect(
      colors.every((color) => /^#[0-9a-f]{6}$/i.test(color)),
      colors.join(' '),
    ).toBe(true)
  })

  it('opens the file picker from Choose custom icon', async () => {
    const click = vi.spyOn(HTMLInputElement.prototype, 'click').mockImplementation(() => {})
    const { user } = renderIcon()
    await user.click(screen.getByRole('button', { name: 'Choose custom icon' }))
    expect(click).toHaveBeenCalledTimes(1)
    click.mockRestore()
  })

  it('previews a picked image at once, and saves it only on Save', async () => {
    const { user, onSave } = renderIcon()
    await user.upload(screen.getByLabelText('Launcher icon image'), png)
    await previewShows(picked)
    expect(onSave).not.toHaveBeenCalled()

    await user.click(screen.getByRole('button', { name: /^Save/ }))
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({ iconChange: { kind: 'custom', file: png, preview: picked } }),
    )
  })

  it('previews the profile’s own icon, and removes it for the generated one', async () => {
    const { user, onSave } = renderIcon({ customIcon: own })
    await previewShows(own)
    expect(screen.getByRole('button', { name: /^Save/ })).toBeDisabled()
    await user.click(screen.getByRole('button', { name: 'Reset to default' }))
    await previewShows(generated)

    await user.click(screen.getByRole('button', { name: /^Save/ }))
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ iconChange: { kind: 'generated' } }))
  })

  it('drops a picked image without anything left to save when the profile has no icon of its own', async () => {
    const { user } = renderIcon()
    await user.upload(screen.getByLabelText('Launcher icon image'), png)
    await user.click(await screen.findByRole('button', { name: 'Reset to default' }))
    await previewShows(generated)
    expect(screen.getByRole('button', { name: /^Save/ })).toBeDisabled()
  })

  it('checks a picked image at once, and turns one away with what is wrong with it', async () => {
    vi.mocked(checkCustomIcon).mockRejectedValueOnce({
      kind: 'Validation',
      message: 'The icon has to be square, and this image is 1200 × 1024.',
    })
    const { user } = renderIcon()
    await previewShows(generated)
    await user.upload(screen.getByLabelText('Launcher icon image'), png)
    expect(await screen.findByText('Could not use this image.')).toBeInTheDocument()
    expect(screen.getByText('The icon has to be square, and this image is 1200 × 1024.')).toBeInTheDocument()
    expect(checkCustomIcon).toHaveBeenCalledWith(new Uint8Array([0x89, 0x50, 0x4e, 0x47]))
    await previewShows(generated)
    expect(screen.getByRole('button', { name: /^Save/ })).toBeDisabled()
  })

  it('shows why a save failed, and keeps the dialog and the picked image', async () => {
    const onClose = vi.fn()
    const { user } = renderIcon({
      onClose,
      onSave: vi.fn().mockRejectedValue({ kind: 'Validation', message: 'ChatGPT (Work) is running. Quit it first.' }),
    })
    await user.upload(screen.getByLabelText('Launcher icon image'), png)
    await previewShows(picked)
    await user.click(screen.getByRole('button', { name: /^Save/ }))
    expect(await screen.findByText('ChatGPT (Work) is running. Quit it first.')).toBeInTheDocument()
    expect(onClose).not.toHaveBeenCalled()
    await previewShows(picked)
  })

  it('drops a picked image on Cancel, so it doesn’t come back looking applied', async () => {
    const onClose = vi.fn()
    const { user } = renderIcon({ onClose })
    await user.upload(screen.getByLabelText('Launcher icon image'), png)
    await previewShows(picked)
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(onClose).toHaveBeenCalled()
    await previewShows(generated)
    expect(screen.getByRole('button', { name: /^Save/ })).toBeDisabled()
  })

  it('shows a placeholder, not the generated icon, until the profile’s own icon is known', async () => {
    renderIcon({ customIcon: undefined })
    await waitFor(() => {
      expect(generatedIconPreview).toHaveBeenCalled()
    })
    expect(screen.queryByRole('img', { name: 'Launcher icon preview' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Reset to default' })).toBeNull()
  })

  it('renders no preview while the dialog is closed', () => {
    vi.mocked(generatedIconPreview).mockClear()
    renderIcon({ open: false })
    expect(generatedIconPreview).not.toHaveBeenCalled()
  })

  it('keeps a reset made while a picked image is still being read', async () => {
    let finishReading: (png: ArrayBuffer) => void = () => {}
    const slow = new File([new Uint8Array([0x89])], 'slow.png', { type: 'image/png' })
    slow.arrayBuffer = () =>
      new Promise((resolve) => {
        finishReading = resolve
      })
    const { user } = renderIcon({ customIcon: own })
    await previewShows(own)
    await user.upload(screen.getByLabelText('Launcher icon image'), slow)
    await user.click(screen.getByRole('button', { name: 'Reset to default' }))
    finishReading(new Uint8Array([0x89]).buffer)
    await previewShows(generated)
    expect(screen.queryByRole('button', { name: 'Reset to default' })).toBeNull()
  })

  it('is unavailable while the desktop launcher is off', async () => {
    const { user } = renderIcon()
    await user.click(screen.getByRole('checkbox', { name: /Desktop App launcher/ }))
    expect(screen.getByRole('button', { name: 'Choose custom icon' })).toBeDisabled()
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
