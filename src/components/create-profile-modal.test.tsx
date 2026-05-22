import type { Dependencies } from '@/lib/types'

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { CreateProfileModal } from './create-profile-modal'

const ALL_INSTALLED: Dependencies = {
  claudeAppInstalled: true,
  claudeCliInstalled: true,
  localBinOnPath: true,
}

function setup(overrides: Partial<Parameters<typeof CreateProfileModal>[0]> = {}) {
  const onClose = vi.fn()
  const onCreate = vi.fn().mockResolvedValue(undefined)
  render(<CreateProfileModal open dependencies={ALL_INSTALLED} onClose={onClose} onCreate={onCreate} {...overrides} />)
  return { onClose, onCreate, user: userEvent.setup() }
}

describe('CreateProfileModal', () => {
  it('disables Create when name is empty', () => {
    setup()
    expect(screen.getByRole('button', { name: 'Create' })).toBeDisabled()
  })

  it('disables Create when no surfaces are selected', async () => {
    const { user } = setup()
    await user.type(screen.getByLabelText('Name'), 'Personal')
    await user.click(screen.getByLabelText(/Desktop app/i))
    await user.click(screen.getByLabelText(/Claude Code CLI/i))
    expect(screen.getByRole('button', { name: 'Create' })).toBeDisabled()
  })

  it('enables Create with a valid name + at least one surface', async () => {
    const { user } = setup()
    await user.type(screen.getByLabelText('Name'), 'Personal')
    expect(screen.getByRole('button', { name: 'Create' })).toBeEnabled()
  })

  it('calls onCreate with trimmed name on submit', async () => {
    const { user, onCreate, onClose } = setup()
    await user.type(screen.getByLabelText('Name'), '  Personal  ')
    await user.click(screen.getByRole('button', { name: 'Create' }))
    expect(onCreate).toHaveBeenCalledWith({
      name: 'Personal',
      color: '#d97757',
      surfaces: { gui: true, cli: true },
    })
    expect(onClose).toHaveBeenCalled()
  })

  it('surfaces backend error in the dialog without closing', async () => {
    const onCreate = vi.fn().mockRejectedValue(new Error('slug exists'))
    const onClose = vi.fn()
    render(<CreateProfileModal open dependencies={ALL_INSTALLED} onClose={onClose} onCreate={onCreate} />)
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Name'), 'Personal')
    await user.click(screen.getByRole('button', { name: 'Create' }))
    expect(await screen.findByText('slug exists')).toBeInTheDocument()
    expect(onClose).not.toHaveBeenCalled()
  })
})

describe('CreateProfileModal — dependency awareness', () => {
  function renderWith(deps: Dependencies) {
    const onCreate = vi.fn().mockResolvedValue(undefined)
    render(<CreateProfileModal open dependencies={deps} onClose={vi.fn()} onCreate={onCreate} />)
    return { onCreate, user: userEvent.setup() }
  }

  it('disables the Desktop surface when Claude.app is missing', () => {
    renderWith({ ...ALL_INSTALLED, claudeAppInstalled: false })
    expect(screen.getByLabelText(/Desktop app launcher/i)).toBeDisabled()
    expect(screen.getByText(/Claude Desktop/)).toBeInTheDocument()
  })

  it('disables the CLI surface when claude is missing and shows the install hint', () => {
    renderWith({ ...ALL_INSTALLED, claudeCliInstalled: false })
    expect(screen.getByLabelText(/Claude Code CLI wrapper/i)).toBeDisabled()
    expect(screen.getByText(/npm install -g @anthropic-ai\/claude-code/)).toBeInTheDocument()
  })

  it('disables submit when both surfaces are unavailable', async () => {
    const { user } = renderWith({
      claudeAppInstalled: false,
      claudeCliInstalled: false,
      localBinOnPath: true,
    })
    await user.type(screen.getByLabelText('Name'), 'Personal')
    expect(screen.getByRole('button', { name: 'Create' })).toBeDisabled()
  })

  it('submits only the available surface when one is missing', async () => {
    const onCreate = vi.fn().mockResolvedValue(undefined)
    render(
      <CreateProfileModal
        open
        dependencies={{ ...ALL_INSTALLED, claudeAppInstalled: false }}
        onClose={vi.fn()}
        onCreate={onCreate}
      />,
    )
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Name'), 'Personal')
    await user.click(screen.getByRole('button', { name: 'Create' }))
    expect(onCreate).toHaveBeenCalledWith({
      name: 'Personal',
      color: '#d97757',
      surfaces: { gui: false, cli: true },
    })
  })
})
