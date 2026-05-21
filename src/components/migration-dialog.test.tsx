import type { ExistingInstallInfo, Profile } from '@/lib/types'

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { MigrationDialog } from './migration-dialog'

const FAKE_PROFILE: Profile = {
  id: '1',
  name: 'Default',
  slug: 'default',
  color: '#7C3AED',
  createdAt: '2026-05-20T12:00:00Z',
  surfaces: { gui: true, cli: true },
}

function setup(existing: ExistingInstallInfo, overrides: Partial<Parameters<typeof MigrationDialog>[0]> = {}) {
  const onClose = vi.fn()
  const onImport = vi.fn().mockResolvedValue(FAKE_PROFILE)
  render(<MigrationDialog open existing={existing} onClose={onClose} onImport={onImport} {...overrides} />)
  return { onClose, onImport, user: userEvent.setup() }
}

describe('MigrationDialog', () => {
  it('renders only the surfaces that were detected', () => {
    setup({
      claudeDesktopPath: '/Users/me/Library/Application Support/Claude',
      claudeCodePath: null,
    })
    expect(screen.getByLabelText(/Desktop app data/i)).toBeInTheDocument()
    expect(screen.queryByLabelText(/Claude Code CLI config/i)).not.toBeInTheDocument()
  })

  it('defaults the name to Default', () => {
    setup({ claudeDesktopPath: '/x', claudeCodePath: '/y' })
    expect(screen.getByLabelText(/Profile name/i)).toHaveValue('Default')
  })

  it('pre-checks every detected surface', () => {
    setup({ claudeDesktopPath: '/x', claudeCodePath: '/y' })
    expect(screen.getByLabelText(/Desktop app data/i)).toBeChecked()
    expect(screen.getByLabelText(/Claude Code CLI config/i)).toBeChecked()
  })

  it('disables Import when both surfaces are unchecked', async () => {
    const { user } = setup({ claudeDesktopPath: '/x', claudeCodePath: '/y' })
    await user.click(screen.getByLabelText(/Desktop app data/i))
    await user.click(screen.getByLabelText(/Claude Code CLI config/i))
    expect(screen.getByRole('button', { name: 'Import' })).toBeDisabled()
  })

  it('shows the CLI re-login warning only when CLI is included', async () => {
    const { user } = setup({ claudeDesktopPath: '/x', claudeCodePath: '/y' })
    expect(screen.getByText(/log in to Claude Code once/i)).toBeInTheDocument()
    await user.click(screen.getByLabelText(/Claude Code CLI config/i))
    expect(screen.queryByText(/log in to Claude Code once/i)).not.toBeInTheDocument()
  })

  it('calls onImport with the trimmed name and selected surfaces', async () => {
    const { user, onImport, onClose } = setup({
      claudeDesktopPath: '/x',
      claudeCodePath: '/y',
    })
    const nameField = screen.getByLabelText(/Profile name/i)
    await user.clear(nameField)
    await user.type(nameField, '  Personal  ')
    await user.click(screen.getByRole('button', { name: 'Import' }))

    expect(onImport).toHaveBeenCalledWith({
      name: 'Personal',
      color: '#7C3AED',
      includeGui: true,
      includeCli: true,
    })
    expect(onClose).toHaveBeenCalled()
  })

  it('surfaces backend errors without closing', async () => {
    const onImport = vi.fn().mockRejectedValue(new Error('disk full'))
    const onClose = vi.fn()
    render(
      <MigrationDialog
        open
        existing={{ claudeDesktopPath: '/x', claudeCodePath: null }}
        onClose={onClose}
        onImport={onImport}
      />,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: 'Import' }))
    expect(await screen.findByText('disk full')).toBeInTheDocument()
    expect(onClose).not.toHaveBeenCalled()
  })
})
