import type { Profile } from '@/lib/types'

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { Sidebar } from './sidebar'

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

describe('Sidebar', () => {
  it('renders one row per profile and marks the selected one active', () => {
    const profiles = [fixture({ id: '1', name: 'Personal' }), fixture({ id: '2', name: 'Work', color: '#6b8db5' })]
    render(<Sidebar profiles={profiles} selectedId="2" onSelect={vi.fn()} onCreate={vi.fn()} onSettings={vi.fn()} />)
    const work = screen.getByRole('button', { name: /Work/ })
    const personal = screen.getByRole('button', { name: /Personal/ })
    expect(work.dataset.active).toBe('true')
    expect(personal.dataset.active).toBe('false')
  })

  it('renders kbd chips matching the profile index (1-based)', () => {
    const profiles = [
      fixture({ id: '1', name: 'Alpha' }),
      fixture({ id: '2', name: 'Bravo' }),
      fixture({ id: '3', name: 'Charlie' }),
    ]
    render(<Sidebar profiles={profiles} selectedId="1" onSelect={vi.fn()} onCreate={vi.fn()} onSettings={vi.fn()} />)
    expect(screen.getByText('⌘1')).toBeInTheDocument()
    expect(screen.getByText('⌘2')).toBeInTheDocument()
    expect(screen.getByText('⌘3')).toBeInTheDocument()
  })

  it('clicking a row fires onSelect with the row id', async () => {
    const onSelect = vi.fn()
    const profiles = [fixture({ id: '7', name: 'Side Project' })]
    render(
      <Sidebar profiles={profiles} selectedId={null} onSelect={onSelect} onCreate={vi.fn()} onSettings={vi.fn()} />,
    )
    await userEvent.setup().click(screen.getByRole('button', { name: /Side Project/ }))
    expect(onSelect).toHaveBeenCalledWith('7')
  })

  it('+ New profile and gear buttons route to their callbacks', async () => {
    const onCreate = vi.fn()
    const onSettings = vi.fn()
    render(<Sidebar profiles={[]} selectedId={null} onSelect={vi.fn()} onCreate={onCreate} onSettings={onSettings} />)
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: /New profile/ }))
    expect(onCreate).toHaveBeenCalledTimes(1)
    await user.click(screen.getByRole('button', { name: 'Open settings' }))
    expect(onSettings).toHaveBeenCalledTimes(1)
  })

  it('search filters the visible rows but keeps the original kbd index', async () => {
    const profiles = [
      fixture({ id: '1', name: 'Personal' }),
      fixture({ id: '2', name: 'Work' }),
      fixture({ id: '3', name: 'Side Project' }),
    ]
    render(<Sidebar profiles={profiles} selectedId="1" onSelect={vi.fn()} onCreate={vi.fn()} onSettings={vi.fn()} />)
    await userEvent.setup().type(screen.getByRole('searchbox'), 'side')
    expect(screen.queryByRole('button', { name: /Personal/ })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Work/ })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Side Project/ })).toBeInTheDocument()
    // Kbd stays ⌘3 because the underlying index is preserved
    expect(screen.getByText('⌘3')).toBeInTheDocument()
  })

  it('surface icons render aria labels reflecting on/off state', () => {
    const profiles = [fixture({ id: '1', name: 'Alpha', surfaces: { gui: true, cli: false } })]
    render(<Sidebar profiles={profiles} selectedId="1" onSelect={vi.fn()} onCreate={vi.fn()} onSettings={vi.fn()} />)
    expect(screen.getByLabelText('Desktop app enabled')).toBeInTheDocument()
    expect(screen.getByLabelText('CLI disabled')).toBeInTheDocument()
  })
})
