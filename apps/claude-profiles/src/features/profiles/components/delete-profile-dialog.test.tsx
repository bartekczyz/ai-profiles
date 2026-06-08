import type { Profile } from '@/lib/types'

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { DeleteProfileDialog } from './delete-profile-dialog'

const fixture: Profile = {
  id: '1',
  app: 'claude',
  name: 'Personal',
  slug: 'personal',
  color: '#d97757',
  createdAt: '2026-05-20T12:00:00Z',
  lastUsedAt: null,

  surfaces: { gui: true, cli: true },
}

describe('DeleteProfileDialog', () => {
  it('defaults the Trash checkbox to checked', () => {
    render(
      <DeleteProfileDialog open profile={fixture} onClose={vi.fn()} onConfirm={vi.fn().mockResolvedValue(undefined)} />,
    )
    expect(screen.getByLabelText(/Move the data directory to Trash/i)).toBeChecked()
  })

  it('calls onConfirm with moveToTrash:true by default', async () => {
    const user = userEvent.setup()
    const onConfirm = vi.fn().mockResolvedValue(undefined)
    render(<DeleteProfileDialog open profile={fixture} onClose={vi.fn()} onConfirm={onConfirm} />)
    await user.click(screen.getByRole('button', { name: /^Delete/ }))
    expect(onConfirm).toHaveBeenCalledWith({ moveToTrash: true })
  })

  it('passes moveToTrash:false when the user unchecks', async () => {
    const user = userEvent.setup()
    const onConfirm = vi.fn().mockResolvedValue(undefined)
    render(<DeleteProfileDialog open profile={fixture} onClose={vi.fn()} onConfirm={onConfirm} />)
    await user.click(screen.getByLabelText(/Move the data directory to Trash/i))
    await user.click(screen.getByRole('button', { name: /^Delete/ }))
    expect(onConfirm).toHaveBeenCalledWith({ moveToTrash: false })
  })

  it('lists only the launchers that exist for the profile', () => {
    render(
      <DeleteProfileDialog
        open
        profile={{ ...fixture, surfaces: { gui: true, cli: false } }}
        onClose={vi.fn()}
        onConfirm={vi.fn().mockResolvedValue(undefined)}
      />,
    )
    expect(screen.getByText(/Claude \(Personal\).app/)).toBeInTheDocument()
    expect(screen.queryByText(/claude-personal/)).not.toBeInTheDocument()
  })

  it('lists the Codex launcher and wrapper for a Codex profile', () => {
    render(
      <DeleteProfileDialog
        open
        profile={{ ...fixture, app: 'codex', name: 'Work', slug: 'work' }}
        onClose={vi.fn()}
        onConfirm={vi.fn().mockResolvedValue(undefined)}
      />,
    )
    expect(screen.getByText(/Codex \(Work\).app/)).toBeInTheDocument()
    expect(screen.getByText(/\.local\/bin\/codex-work/)).toBeInTheDocument()
  })
})
