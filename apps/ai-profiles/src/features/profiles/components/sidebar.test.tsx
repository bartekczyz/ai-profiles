import type { Profile, SidebarEntry } from '@/lib/types'

import { render, screen, within } from '@testing-library/react'
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
      distinctDockIcon: false,
      lastUsedAt: null,
      surfaces: { gui: true, cli: true },
      ...overrides,
    },
  }
}

function defaultEntry(app: 'claude' | 'codex' = 'claude'): SidebarEntry {
  return {
    kind: 'default',
    entry: {
      id: `default:${app}`,
      app,
      name: app === 'codex' ? 'Codex' : 'Default',
      customName: null,
      color: null,
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

  it('gives the default row ⌘1', () => {
    render(
      <Sidebar
        entries={[defaultEntry()]}
        selectedId={null}
        onSelect={vi.fn()}
        onCreate={vi.fn()}
        onSettings={vi.fn()}
      />,
    )
    expect(screen.getByText('⌘1')).toBeInTheDocument()
  })

  it('numbers the default row first and the managed rows after it', () => {
    render(
      <Sidebar
        entries={[defaultEntry(), managedEntry({ id: '1', name: 'Personal' }), managedEntry({ id: '2', name: 'Work' })]}
        selectedId={null}
        onSelect={vi.fn()}
        onCreate={vi.fn()}
        onSettings={vi.fn()}
      />,
    )
    const chipIn = (name: string) =>
      within(screen.getByText(name).closest('[aria-keyshortcuts]') as HTMLElement).getByText(/⌘\d/).textContent
    expect(chipIn('Default')).toBe('⌘1')
    expect(chipIn('Personal')).toBe('⌘2')
    expect(chipIn('Work')).toBe('⌘3')
  })

  it('shows app glyphs when entries span both Claude and Codex', () => {
    const entries = [
      defaultEntry('claude'),
      defaultEntry('codex'),
      managedEntry({ id: '1', name: 'Personal', app: 'claude' }),
    ]
    render(<Sidebar entries={entries} selectedId={null} onSelect={vi.fn()} onCreate={vi.fn()} onSettings={vi.fn()} />)
    // At least one glyph must be present (the data-app-glyph attribute is set by AppGlyph)
    const glyphs = document.querySelectorAll('[data-app-glyph]')
    expect(glyphs.length).toBeGreaterThan(0)
  })

  it('does not show app glyphs when all entries belong to one app', () => {
    const entries = [defaultEntry('claude'), managedEntry({ id: '1', name: 'Personal', app: 'claude' })]
    render(<Sidebar entries={entries} selectedId={null} onSelect={vi.fn()} onCreate={vi.fn()} onSettings={vi.fn()} />)
    const glyphs = document.querySelectorAll('[data-app-glyph]')
    expect(glyphs.length).toBe(0)
  })

  // The per-app text headers are gone, so grouping is now carried entirely by
  // row order plus a glyph on every row. The two tests above only prove a
  // glyph exists somewhere — which was equally true when it lived in the
  // header — so these cover what actually replaced it.
  it('gives every row its own app glyph once the sidebar spans two apps', () => {
    const entries = [
      defaultEntry('claude'),
      managedEntry({ id: '1', name: 'Personal', app: 'claude' }),
      defaultEntry('codex'),
      managedEntry({ id: '2', name: 'Research', app: 'codex' }),
    ]
    render(<Sidebar entries={entries} selectedId={null} onSelect={vi.fn()} onCreate={vi.fn()} onSettings={vi.fn()} />)
    expect(document.querySelectorAll('[data-app-glyph]')).toHaveLength(entries.length)
  })

  it('keeps each app group contiguous and in registry order', () => {
    const entries = [
      defaultEntry('claude'),
      managedEntry({ id: '1', name: 'Personal', app: 'claude' }),
      defaultEntry('codex'),
      managedEntry({ id: '2', name: 'Research', app: 'codex' }),
    ]
    render(<Sidebar entries={entries} selectedId={null} onSelect={vi.fn()} onCreate={vi.fn()} onSettings={vi.fn()} />)
    const rendered = screen.getAllByRole('button').map((button) => button.textContent ?? '')
    const personal = rendered.findIndex((label) => label.includes('Personal'))
    const research = rendered.findIndex((label) => label.includes('Research'))
    expect(personal).toBeGreaterThanOrEqual(0)
    expect(research).toBeGreaterThan(personal)
  })
})
