import type { Profile } from '@/lib/types'

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { EditProfileModal } from './edit-profile-modal'

function fixture(overrides: Partial<Profile> = {}): Profile {
  return {
    id: '1',
    name: 'Personal',
    slug: 'personal',
    color: '#d97757',
    createdAt: '2026-05-20T12:00:00Z',
    surfaces: { gui: true, cli: true },
    ...overrides,
  }
}

describe('EditProfileModal', () => {
  it('disables Save when nothing has changed', () => {
    const onSave = vi.fn().mockResolvedValue(undefined)
    render(<EditProfileModal open profile={fixture()} onClose={vi.fn()} onSave={onSave} />)
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled()
  })

  it('enables Save when the name changes', async () => {
    const user = userEvent.setup()
    render(
      <EditProfileModal open profile={fixture()} onClose={vi.fn()} onSave={vi.fn().mockResolvedValue(undefined)} />,
    )
    const input = screen.getByLabelText('Name')
    await user.clear(input)
    await user.type(input, 'Renamed')
    expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled()
  })

  it('resets form state when a different profile is opened', () => {
    const { rerender } = render(
      <EditProfileModal
        open
        profile={fixture({ id: '1', name: 'Personal' })}
        onClose={vi.fn()}
        onSave={vi.fn().mockResolvedValue(undefined)}
      />,
    )

    rerender(
      <EditProfileModal
        open
        profile={fixture({ id: '2', name: 'Work' })}
        onClose={vi.fn()}
        onSave={vi.fn().mockResolvedValue(undefined)}
      />,
    )

    expect(screen.getByLabelText('Name')).toHaveValue('Work')
  })

  it('calls onSave with the trimmed name', async () => {
    const user = userEvent.setup()
    const onSave = vi.fn().mockResolvedValue(undefined)
    const onClose = vi.fn()
    render(<EditProfileModal open profile={fixture()} onClose={onClose} onSave={onSave} />)
    const input = screen.getByLabelText('Name')
    await user.clear(input)
    await user.type(input, '  Renamed  ')
    await user.click(screen.getByRole('button', { name: 'Save' }))
    expect(onSave).toHaveBeenCalledWith({ name: 'Renamed', color: '#d97757' })
  })
})
