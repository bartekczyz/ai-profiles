import type { ExistingInstallInfo, Profile } from '@/lib/types'

import { invoke } from '@tauri-apps/api/core'
import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { renderWithQuery } from '@/test/render-with-query'

import { MigrationDialog } from './migration-dialog'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const mockInvoke = vi.mocked(invoke)

beforeEach(() => {
  mockInvoke.mockReset()
})

const FAKE_PROFILE: Profile = {
  id: '1',
  name: 'Default',
  slug: 'default',
  color: '#d97757',
  createdAt: '2026-05-20T12:00:00Z',
  lastUsedAt: null,

  surfaces: { gui: true, cli: true },
}

function existing(overrides: Partial<ExistingInstallInfo> = {}): ExistingInstallInfo {
  return {
    claudeDesktopPath: '/Users/me/Library/Application Support/Claude',
    claudeCodePath: '/Users/me/.claude',
    claudeDesktopSizeBytes: 248 * 1024 * 1024,
    claudeCodeSizeBytes: 4 * 1024 * 1024,
    ...overrides,
  }
}

function setup(detected: ExistingInstallInfo, overrides: Partial<Parameters<typeof MigrationDialog>[0]> = {}) {
  // The dialog's lazy size query fires once it opens; resolve it
  // immediately with the sizes the caller embedded in `existing`. Tests
  // that override these (e.g. "omits the size span when size is missing")
  // get the matching pre-stocked response.
  mockInvoke.mockResolvedValue({
    claudeDesktopSizeBytes: detected.claudeDesktopSizeBytes,
    claudeCodeSizeBytes: detected.claudeCodeSizeBytes,
  })
  const onClose = vi.fn()
  const onImport = vi.fn().mockResolvedValue(FAKE_PROFILE)
  renderWithQuery(<MigrationDialog open existing={detected} onClose={onClose} onImport={onImport} {...overrides} />)
  return { onClose, onImport, user: userEvent.setup() }
}

describe('MigrationDialog', () => {
  it('renders only the surfaces that were detected', () => {
    setup(existing({ claudeCodePath: null, claudeCodeSizeBytes: null }))
    expect(screen.getByLabelText(/Desktop app data/i)).toBeInTheDocument()
    expect(screen.queryByLabelText(/Claude Code CLI config/i)).not.toBeInTheDocument()
  })

  it('defaults the name to Default', () => {
    setup(existing())
    expect(screen.getByLabelText(/Profile name/i)).toHaveValue('Default')
  })

  it('pre-checks every detected surface', () => {
    setup(existing())
    expect(screen.getByLabelText(/Desktop app data/i)).toBeChecked()
    expect(screen.getByLabelText(/Claude Code CLI config/i)).toBeChecked()
  })

  it('shows formatted sizes alongside each detected path', async () => {
    setup(existing())
    // Sizes arrive via the lazy `detect_existing_claude_sizes` query
    // that fires when the dialog opens — wait for it.
    expect(await screen.findByText(/248 MB/)).toBeInTheDocument()
    expect(await screen.findByText(/4\.0 MB/)).toBeInTheDocument()
  })

  it('omits the size span when size is missing', () => {
    setup(existing({ claudeDesktopSizeBytes: null, claudeCodeSizeBytes: null }))
    expect(screen.queryByText(/MB/)).not.toBeInTheDocument()
  })

  it('disables Import when both surfaces are unchecked', async () => {
    const { user } = setup(existing())
    await user.click(screen.getByLabelText(/Desktop app data/i))
    await user.click(screen.getByLabelText(/Claude Code CLI config/i))
    expect(screen.getByRole('button', { name: /^Import/ })).toBeDisabled()
  })

  it('calls onImport with the trimmed name and selected surfaces', async () => {
    const { user, onImport, onClose } = setup(existing())
    const nameField = screen.getByLabelText(/Profile name/i)
    await user.clear(nameField)
    await user.type(nameField, '  Personal  ')
    await user.click(screen.getByRole('button', { name: /^Import/ }))

    expect(onImport).toHaveBeenCalledWith({
      name: 'Personal',
      color: '#d97757',
      includeGui: true,
      includeCli: true,
    })
    expect(onClose).toHaveBeenCalled()
  })

  it('surfaces backend errors without closing', async () => {
    mockInvoke.mockResolvedValue({ claudeDesktopSizeBytes: null, claudeCodeSizeBytes: null })
    const onImport = vi.fn().mockRejectedValue(new Error('disk full'))
    const onClose = vi.fn()
    renderWithQuery(
      <MigrationDialog
        open
        existing={existing({ claudeCodePath: null, claudeCodeSizeBytes: null })}
        onClose={onClose}
        onImport={onImport}
      />,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: /^Import/ }))
    expect(await screen.findByText('disk full')).toBeInTheDocument()
    expect(onClose).not.toHaveBeenCalled()
  })
})
