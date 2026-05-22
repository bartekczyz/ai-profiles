import type { Dependencies, Profile } from '@/lib/types'

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'

import { EditProfileDialog } from './edit-profile-dialog'

type DialogProps = Parameters<typeof EditProfileDialog>[0]

function renderEdit(props: DialogProps) {
  return render(
    <ToastProvider>
      <EditProfileDialog {...props} />
    </ToastProvider>,
  )
}

function fixture(overrides: Partial<Profile> = {}): Profile {
  return {
    id: '1',
    name: 'Personal',
    slug: 'personal',
    color: '#d97757',
    createdAt: '2026-05-20T12:00:00Z',
    lastUsedAt: null,

    surfaces: { gui: true, cli: true },
    ...overrides,
  }
}

const DEPS: Dependencies = {
  claudeAppInstalled: true,
  claudeCliInstalled: true,
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
    expect(onSave).toHaveBeenCalledWith({ name: 'Renamed', color: '#d97757', surfaces: { gui: true, cli: true } })
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
    expect(onSave).toHaveBeenCalledWith({ name: 'Renamed', color: '#d97757', surfaces: { gui: true, cli: true } })
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
