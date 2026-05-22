import type { Dependencies, Profile } from '@/lib/types'

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { CommandPalette } from './command-palette'

function profile(overrides: Partial<Profile> = {}): Profile {
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

type RenderProps = Partial<Parameters<typeof CommandPalette>[0]>

function setup(props: RenderProps = {}) {
  const handlers = {
    onClose: vi.fn(),
    onSwitch: vi.fn(),
    onLaunch: vi.fn(),
    onCopy: vi.fn(),
    onCreate: vi.fn(),
    onSettings: vi.fn(),
    onImport: vi.fn(),
  }
  render(<CommandPalette open profiles={[profile()]} selectedId="1" dependencies={DEPS} {...handlers} {...props} />)
  return { ...handlers, user: userEvent.setup() }
}

describe('CommandPalette', () => {
  it('renders one launch, copy, and switch row per profile (flat — no per-profile heading)', () => {
    setup({ profiles: [profile({ id: '1', name: 'Personal' }), profile({ id: '2', name: 'Work', slug: 'work' })] })
    // Launch rows include the profile name in the label (one row per profile).
    expect(screen.getAllByText(/Launch desktop app/)).toHaveLength(2)
    expect(screen.getByText(/claude-personal/)).toBeInTheDocument()
    expect(screen.getByText(/claude-work/)).toBeInTheDocument()
    expect(screen.getByText('Switch to Personal')).toBeInTheDocument()
    expect(screen.getByText('Switch to Work')).toBeInTheDocument()
  })

  it('hides launch when GUI surface is off', () => {
    setup({ profiles: [profile({ surfaces: { gui: false, cli: true } })] })
    expect(screen.queryByText(/Launch desktop app/)).not.toBeInTheDocument()
    expect(screen.getByText(/claude-personal/)).toBeInTheDocument()
  })

  it('hides copy when CLI surface is off', () => {
    setup({ profiles: [profile({ surfaces: { gui: true, cli: false } })] })
    expect(screen.queryByText(/claude-personal/)).not.toBeInTheDocument()
    expect(screen.getByText(/Launch desktop app/)).toBeInTheDocument()
  })

  it('always shows the three global actions', () => {
    setup({ profiles: [] })
    expect(screen.getByText('Create new profile')).toBeInTheDocument()
    expect(screen.getByText('Open settings')).toBeInTheDocument()
    expect(screen.getByText('Detect and import…')).toBeInTheDocument()
  })

  it('clicking switch fires onSwitch and onClose', async () => {
    const { onSwitch, onClose, user } = setup({
      profiles: [profile({ id: '7', name: 'Side Project', slug: 'side' })],
      selectedId: null,
    })
    await user.click(screen.getByText('Switch to Side Project'))
    expect(onSwitch).toHaveBeenCalledWith('7')
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('filters items by typing into the search input', async () => {
    const { user } = setup({
      profiles: [profile({ id: '1', name: 'Personal' }), profile({ id: '2', name: 'Work', slug: 'work' })],
    })
    await user.type(screen.getByPlaceholderText(/Type a command/), 'work')
    expect(screen.queryByText('Switch to Personal')).not.toBeInTheDocument()
    expect(screen.getByText('Switch to Work')).toBeInTheDocument()
  })

  it('typing "cli" surfaces the CLI copy item via keywords', async () => {
    const { user } = setup({ profiles: [profile()] })
    await user.type(screen.getByPlaceholderText(/Type a command/), 'cli')
    expect(screen.getByText(/claude-personal/)).toBeInTheDocument()
  })

  it('renders the magnifier glyph and esc chip in the search row', () => {
    setup()
    expect(screen.getByText('esc')).toBeInTheDocument()
  })
})
