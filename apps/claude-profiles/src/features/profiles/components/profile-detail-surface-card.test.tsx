import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { ProfileDetailSurfaceCard } from './profile-detail-surface-card'

describe('ProfileDetailSurfaceCard', () => {
  it('renders Desktop App copy + primary CTA for the gui variant', () => {
    render(
      <ProfileDetailSurfaceCard
        variant="gui"
        app="claude"
        enabled
        primaryLabel="Open Claude"
        primaryKbd="⏎"
        statusDetail="Launcher ready in /Applications/Claude.app"
        secondaries={[
          { label: 'Reveal app', kbd: '⌥1', onClick: vi.fn() },
          { label: 'Launcher', kbd: '⌥2', onClick: vi.fn() },
        ]}
        onPrimary={vi.fn()}
      />,
    )
    expect(screen.getByRole('heading', { name: 'Desktop App' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Open Claude/ })).toBeInTheDocument()
    expect(screen.getByText('⌥1')).toBeInTheDocument()
    expect(screen.getByText('⌥2')).toBeInTheDocument()
  })

  it('describes the Claude CLI surface for the cli variant', () => {
    render(
      <ProfileDetailSurfaceCard
        variant="cli"
        app="claude"
        enabled
        primaryLabel="Copy claude-personal"
        primaryKbd="⌘C"
        statusDetail="Wrapper installed"
        secondaries={[
          { label: 'Config', kbd: '⌥3', onClick: vi.fn() },
          { label: 'Wrapper', kbd: '⌥4', onClick: vi.fn() },
        ]}
        onPrimary={vi.fn()}
      />,
    )
    expect(screen.getByText(/CLAUDE_CONFIG_DIR/)).toBeInTheDocument()
    expect(screen.getByText(/claude binary/)).toBeInTheDocument()
  })

  it('describes the Codex CLI surface (binary, CODEX_HOME, codex-{slug}) for a Codex profile', () => {
    render(
      <ProfileDetailSurfaceCard
        variant="cli"
        app="codex"
        enabled
        primaryLabel="Copy codex-personal"
        primaryKbd="⌘C"
        statusDetail="Wrapper installed"
        secondaries={[]}
        onPrimary={vi.fn()}
      />,
    )
    expect(screen.getByText(/codex binary/)).toBeInTheDocument()
    expect(screen.getByText(/CODEX_HOME/)).toBeInTheDocument()
    expect(screen.getByText(/codex-\{slug\}/)).toBeInTheDocument()
  })

  it('disabled state dims the card and disables every action', () => {
    const onPrimary = vi.fn()
    const onReveal = vi.fn()
    render(
      <ProfileDetailSurfaceCard
        variant="gui"
        app="claude"
        enabled={false}
        primaryLabel="Open Claude"
        primaryKbd="⏎"
        statusDetail="Launcher ready"
        secondaries={[{ label: 'Reveal app', kbd: '⌥1', onClick: onReveal }]}
        onPrimary={onPrimary}
      />,
    )
    expect(screen.getByText(/Surface disabled/)).toBeInTheDocument()
    const primary = screen.getByRole('button', { name: /Open Claude/ })
    const reveal = screen.getByRole('button', { name: /Reveal app/ })
    expect(primary).toBeDisabled()
    expect(reveal).toBeDisabled()
  })

  it('clicking the primary CTA fires onPrimary', async () => {
    const onPrimary = vi.fn()
    render(
      <ProfileDetailSurfaceCard
        variant="gui"
        app="claude"
        enabled
        primaryLabel="Open Claude"
        primaryKbd="⏎"
        statusDetail="Launcher ready"
        secondaries={[]}
        onPrimary={onPrimary}
      />,
    )
    await userEvent.setup().click(screen.getByRole('button', { name: /Open Claude/ }))
    expect(onPrimary).toHaveBeenCalledTimes(1)
  })
})
