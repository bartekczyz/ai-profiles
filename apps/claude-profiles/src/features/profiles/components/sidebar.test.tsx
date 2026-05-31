import type { Profile, SidebarEntry } from '@/lib/types'

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { Sidebar } from './sidebar'

function managedEntry(overrides: Partial<Profile> = {}): SidebarEntry {
  return {
    kind: 'managed',
    profile: {
      id: '1',
      app: 'claude',
      name: 'Personal',
      slug: 'personal',
      color: '#d97757',
      createdAt: '2026-05-20T12:00:00Z',
      lastUsedAt: null,
      surfaces: { gui: true, cli: true },
      ...overrides,
    },
  }
}

function defaultEntry(): SidebarEntry {
  return {
    kind: 'default',
    entry: {
      id: 'default:claude',
      app: 'claude',
      name: 'Default',
      surfaces: { gui: true, cli: true },
    },
  }
}

describe('Sidebar', () => {
  it('renders one row per entry and marks the selected one active', () => {
    const entries = [
      managedEntry({ id: '1', name: 'Personal' }),
      managedEntry({ id: '2', name: 'Work', color: '#6b8db5' }),
    ]
    render(<Sidebar entries={entries} selectedId="2" onSelect={vi.fn()} onCreate={vi.fn()} onSettings={vi.fn()} />)
    const work = screen.getByRole('button', { name: /Work/ })
    const personal = screen.getByRole('button', { name: /Personal/ })
    expect(work.dataset.active).toBe('true')
    expect(personal.dataset.active).toBe('false')
  })

  it('renders kbd chips matching the managed profile index (1-based)', () => {
    const entries = [
      managedEntry({ id: '1', name: 'Alpha' }),
      managedEntry({ id: '2', name: 'Bravo' }),
      managedEntry({ id: '3', name: 'Charlie' }),
    ]
    render(<Sidebar entries={entries} selectedId="1" onSelect={vi.fn()} onCreate={vi.fn()} onSettings={vi.fn()} />)
    expect(screen.getByText('⌘1')).toBeInTheDocument()
    expect(screen.getByText('⌘2')).toBeInTheDocument()
    expect(screen.getByText('⌘3')).toBeInTheDocument()
  })

  it('clicking a row fires onSelect with the row id', async () => {
    const onSelect = vi.fn()
    const entries = [managedEntry({ id: '7', name: 'Side Project' })]
    render(<Sidebar entries={entries} selectedId={null} onSelect={onSelect} onCreate={vi.fn()} onSettings={vi.fn()} />)
    await userEvent.setup().click(screen.getByRole('button', { name: /Side Project/ }))
    expect(onSelect).toHaveBeenCalledWith('7')
  })

  it('+ New profile and gear buttons route to their callbacks', async () => {
    const onCreate = vi.fn()
    const onSettings = vi.fn()
    render(<Sidebar entries={[]} selectedId={null} onSelect={vi.fn()} onCreate={onCreate} onSettings={onSettings} />)
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: /New profile/ }))
    expect(onCreate).toHaveBeenCalledTimes(1)
    await user.click(screen.getByRole('button', { name: 'Open settings' }))
    expect(onSettings).toHaveBeenCalledTimes(1)
  })

  it('search filters the visible rows but keeps the original kbd index', async () => {
    const entries = [
      managedEntry({ id: '1', name: 'Personal' }),
      managedEntry({ id: '2', name: 'Work' }),
      managedEntry({ id: '3', name: 'Side Project' }),
    ]
    render(<Sidebar entries={entries} selectedId="1" onSelect={vi.fn()} onCreate={vi.fn()} onSettings={vi.fn()} />)
    await userEvent.setup().type(screen.getByRole('searchbox'), 'side')
    expect(screen.queryByRole('button', { name: /Personal/ })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Work/ })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Side Project/ })).toBeInTheDocument()
    // Kbd stays ⌘3 because the underlying index is preserved
    expect(screen.getByText('⌘3')).toBeInTheDocument()
  })

  it('surface icons render aria labels reflecting on/off state', () => {
    const entries = [managedEntry({ id: '1', name: 'Alpha', surfaces: { gui: true, cli: false } })]
    render(<Sidebar entries={entries} selectedId="1" onSelect={vi.fn()} onCreate={vi.fn()} onSettings={vi.fn()} />)
    expect(screen.getByLabelText('Desktop app enabled')).toBeInTheDocument()
    expect(screen.getByLabelText('CLI disabled')).toBeInTheDocument()
  })

  it('renders the default row before the managed rows', () => {
    render(
      <Sidebar
        entries={[defaultEntry(), managedEntry({ id: '1', name: 'Personal' })]}
        selectedId={null}
        onSelect={vi.fn()}
        onCreate={vi.fn()}
        onSettings={vi.fn()}
      />,
    )
    const buttons = screen.getAllByRole('button')
    const defaultIndex = buttons.findIndex((button) => button.textContent?.includes('Default'))
    const personalIndex = buttons.findIndex((button) => button.textContent?.includes('Personal'))
    expect(defaultIndex).toBeGreaterThanOrEqual(0)
    expect(defaultIndex).toBeLessThan(personalIndex)
  })

  it('does not render a ⌘ shortcut chip on the default row', () => {
    render(
      <Sidebar
        entries={[defaultEntry()]}
        selectedId={null}
        onSelect={vi.fn()}
        onCreate={vi.fn()}
        onSettings={vi.fn()}
      />,
    )
    expect(screen.queryByText(/⌘\d/)).toBeNull()
  })

  it('managed rows keep ⌘1..⌘N numbering regardless of the default row', () => {
    render(
      <Sidebar
        entries={[defaultEntry(), managedEntry({ id: '1', name: 'Personal' }), managedEntry({ id: '2', name: 'Work' })]}
        selectedId={null}
        onSelect={vi.fn()}
        onCreate={vi.fn()}
        onSettings={vi.fn()}
      />,
    )
    expect(screen.getByText('⌘1')).toBeInTheDocument()
    expect(screen.getByText('⌘2')).toBeInTheDocument()
  })
})
