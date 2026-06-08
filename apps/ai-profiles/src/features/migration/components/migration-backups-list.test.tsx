import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { MigrationBackupsList } from './migration-backups-list'

const FIXTURE = [
  {
    path: '/x/migration-backup-1',
    createdAtMs: Date.now() - 10 * 24 * 60 * 60 * 1000,
    sizeBytes: 5 * 1024 * 1024,
    eligibleForCleanup: true,
  },
  {
    path: '/x/migration-backup-2',
    createdAtMs: Date.now() - 60 * 60 * 1000,
    sizeBytes: 800,
    eligibleForCleanup: false,
  },
]

describe('MigrationBackupsList', () => {
  it('shows an empty-state card when no backups exist', () => {
    render(<MigrationBackupsList backups={[]} onDelete={vi.fn().mockResolvedValue(undefined)} />)
    expect(screen.getByText(/No migration backups/i)).toBeInTheDocument()
  })

  it('renders one row per backup using the path basename, size, and age', () => {
    render(<MigrationBackupsList backups={FIXTURE} onDelete={vi.fn().mockResolvedValue(undefined)} />)
    expect(screen.getByText('migration-backup-1')).toBeInTheDocument()
    expect(screen.getByText('migration-backup-2')).toBeInTheDocument()
    expect(screen.getByText('5.0 MB')).toBeInTheDocument()
    expect(screen.getByText('800 B')).toBeInTheDocument()
    expect(screen.getByText('10 days old')).toBeInTheDocument()
    expect(screen.getByText(/1 hour old/)).toBeInTheDocument()
  })

  it('calls onDelete with the row path when Delete is clicked', async () => {
    const onDelete = vi.fn().mockResolvedValue(undefined)
    render(<MigrationBackupsList backups={FIXTURE} onDelete={onDelete} />)
    const user = userEvent.setup()
    const deleteButtons = screen.getAllByRole('button', { name: 'Delete' })
    await user.click(deleteButtons[0])
    expect(onDelete).toHaveBeenCalledWith('/x/migration-backup-1')
  })

  it('surfaces delete errors inline', async () => {
    const onDelete = vi.fn().mockRejectedValue(new Error('permission denied'))
    render(<MigrationBackupsList backups={FIXTURE} onDelete={onDelete} />)
    const user = userEvent.setup()
    await user.click(screen.getAllByRole('button', { name: 'Delete' })[0])
    expect(await screen.findByText('permission denied')).toBeInTheDocument()
  })
})
