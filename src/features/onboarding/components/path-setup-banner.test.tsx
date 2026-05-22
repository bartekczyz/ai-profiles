import { invoke } from '@tauri-apps/api/core'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { PathSetupBanner } from './path-setup-banner'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const mockInvoke = vi.mocked(invoke)

beforeEach(() => {
  mockInvoke.mockReset()
})

describe('PathSetupBanner', () => {
  it('renders the explainer copy', () => {
    render(<PathSetupBanner onFixed={vi.fn()} onDismiss={vi.fn()} />)
    expect(screen.getByText(/Your shell doesn't look in/i)).toBeInTheDocument()
  })

  it('calls onDismiss when "Not now" is clicked', async () => {
    const onDismiss = vi.fn()
    render(<PathSetupBanner onFixed={vi.fn()} onDismiss={onDismiss} />)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Not now' }))
    expect(onDismiss).toHaveBeenCalled()
  })

  it('shows the "updated" success message after installing the hook', async () => {
    mockInvoke.mockResolvedValueOnce('zsh')
    mockInvoke.mockResolvedValueOnce({
      outcome: 'installed',
      rcPath: '/Users/test/.zshrc',
      backupPath: '/Users/test/.zshrc.claude-profiles-backup-1',
    })
    const onFixed = vi.fn()
    render(<PathSetupBanner onFixed={onFixed} onDismiss={vi.fn()} />)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Fix it for me' }))
    expect(await screen.findByText(/Updated ~\/\.zshrc/)).toBeInTheDocument()
    expect(onFixed).toHaveBeenCalled()
  })

  it('shows the "already installed" success message when the hook was present', async () => {
    mockInvoke.mockResolvedValueOnce('zsh')
    mockInvoke.mockResolvedValueOnce({
      outcome: 'alreadyInstalled',
      rcPath: '/Users/test/.zshrc',
    })
    render(<PathSetupBanner onFixed={vi.fn()} onDismiss={vi.fn()} />)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Fix it for me' }))
    expect(await screen.findByText(/already has the line/i)).toBeInTheDocument()
  })

  it('surfaces errors from install_path_hook', async () => {
    mockInvoke.mockResolvedValueOnce('zsh')
    mockInvoke.mockRejectedValueOnce(new Error('permission denied'))
    render(<PathSetupBanner onFixed={vi.fn()} onDismiss={vi.fn()} />)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Fix it for me' }))
    expect(await screen.findByText('permission denied')).toBeInTheDocument()
  })
})
