import type { ReactElement } from 'react'
import type { SessionRowAction } from './session-row-actions'

import { render as renderUnwrapped, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { TooltipProvider } from '@/design/ui/tooltip'

import { SessionRowActions } from './session-row-actions'

/**
 * One enabled action and one blocked with a reason.
 */
function makeActions(): Array<SessionRowAction> {
  return [
    { id: 'archive', label: 'Archive', onSelect: vi.fn() },
    { id: 'move', label: 'Move', disabledReason: 'Close it in the terminal first', onSelect: vi.fn() },
  ]
}

/**
 * Renders `ui` under the tooltip provider the list puts above its rows.
 */
function render(ui: ReactElement) {
  return renderUnwrapped(ui, { wrapper: TooltipProvider })
}

// Both presentations are in the DOM at once — a container query decides
// which one shows, and the test DOM applies no CSS — so each test drives one.
describe('SessionRowActions — inline', () => {
  it('runs an enabled action', async () => {
    const actions = makeActions()
    render(<SessionRowActions actions={actions} />)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Archive' }))
    expect(actions[0].onSelect).toHaveBeenCalledOnce()
  })

  it('blocks a disabled action and says why', async () => {
    const actions = makeActions()
    render(<SessionRowActions actions={actions} />)
    const move = screen.getByRole('button', { name: 'Move' })
    expect(move).toHaveAttribute('aria-disabled', 'true')
    expect(move).toHaveAccessibleDescription('Close it in the terminal first')
    await userEvent.setup().click(move)
    expect(actions[1].onSelect).not.toHaveBeenCalled()
  })

  it('keeps focus on an action as it is held back and let go again', async () => {
    const archive: SessionRowAction = { id: 'archive', label: 'Archive', onSelect: vi.fn() }
    const { rerender } = render(<SessionRowActions actions={[archive]} />)
    await userEvent.setup().tab()
    expect(screen.getByRole('button', { name: 'Archive' })).toHaveFocus()

    rerender(<SessionRowActions actions={[{ ...archive, disabledReason: 'Close it in the terminal first' }]} />)
    expect(screen.getByRole('button', { name: 'Archive' })).toHaveFocus()

    rerender(<SessionRowActions actions={[archive]} />)
    expect(screen.getByRole('button', { name: 'Archive' })).toHaveFocus()
  })

  it('keeps focus on an action with targets as it is held back', async () => {
    const move: SessionRowAction = {
      id: 'move',
      label: 'Move',
      targets: [{ id: 'p2', label: 'Personal' }],
      onSelect: vi.fn(),
    }
    const { rerender } = render(<SessionRowActions actions={[move]} />)
    await userEvent.setup().tab()
    expect(screen.getByRole('button', { name: 'Move' })).toHaveFocus()

    rerender(<SessionRowActions actions={[{ ...move, disabledReason: 'Close it in the terminal first' }]} />)
    expect(screen.getByRole('button', { name: 'Move' })).toHaveFocus()
  })

  it('announces a menu only on an action whose menu can open', () => {
    const move: SessionRowAction = {
      id: 'move',
      label: 'Move',
      targets: [{ id: 'p2', label: 'Personal' }],
      onSelect: vi.fn(),
    }
    const { rerender } = render(<SessionRowActions actions={[move]} />)
    expect(screen.getByRole('button', { name: 'Move' })).toHaveAttribute('aria-haspopup', 'menu')
    expect(screen.getByRole('button', { name: 'Move' })).toHaveAttribute('aria-expanded', 'false')

    rerender(<SessionRowActions actions={[{ ...move, disabledReason: 'Close it in the terminal first' }]} />)
    expect(screen.getByRole('button', { name: 'Move' })).not.toHaveAttribute('aria-haspopup')
    expect(screen.getByRole('button', { name: 'Move' })).not.toHaveAttribute('aria-expanded')
  })

  it('shows why on hover, outside the row, where a scrolling list can’t clip it', async () => {
    const { container } = render(<SessionRowActions actions={makeActions()} />)
    await userEvent.setup().hover(screen.getByRole('button', { name: 'Move' }))
    const tooltip = await screen.findByRole('tooltip')
    expect(tooltip).toHaveTextContent('Close it in the terminal first')
    expect(container).not.toContainElement(tooltip)
  })
})

describe('SessionRowActions — overflow menu', () => {
  it('runs an enabled action from the menu', async () => {
    const actions = makeActions()
    const user = userEvent.setup()
    render(<SessionRowActions actions={actions} />)
    await user.click(screen.getByRole('button', { name: 'Session actions' }))
    await user.click(screen.getByRole('menuitem', { name: 'Archive' }))
    expect(actions[0].onSelect).toHaveBeenCalledOnce()
  })

  it('offers a disabled action greyed out, with its reason', async () => {
    const actions = makeActions()
    const user = userEvent.setup()
    render(<SessionRowActions actions={actions} />)
    await user.click(screen.getByRole('button', { name: 'Session actions' }))
    const move = screen.getByRole('menuitem', { name: /Move/ })
    expect(move).toHaveAttribute('aria-disabled', 'true')
    expect(move).toHaveTextContent('Close it in the terminal first')
    await user.click(move)
    expect(actions[1].onSelect).not.toHaveBeenCalled()
  })
})
