import type { Profile, SessionSummary, TransferPlan } from '@/lib/types'

import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import {
  archiveSession,
  checkSessionArchive,
  listProfiles,
  listSessions,
  planSessionTransfer,
  transferSession,
} from '@/lib/commands'
import { renderWithQuery } from '@/test/render-with-query'

import { ProfileDetailSessions } from './profile-detail-sessions'

vi.mock('@/lib/commands', async () => {
  const actual = await vi.importActual<typeof import('@/lib/commands')>('@/lib/commands')
  return {
    ...actual,
    listProfiles: vi.fn(),
    listSessions: vi.fn(),
    archiveSession: vi.fn(),
    checkSessionArchive: vi.fn(),
    planSessionTransfer: vi.fn(),
    transferSession: vi.fn(),
  }
})

function profile(id: string, name: string, app: Profile['app'] = 'claude'): Profile {
  return {
    id,
    app,
    name,
    slug: name.toLowerCase(),
    color: '#d97757',
    createdAt: '2026-01-01T00:00:00Z',
    surfaces: { gui: true, cli: true },
    distinctDockIcon: false,
    lastUsedAt: null,
  }
}

function session(overrides: Partial<SessionSummary> = {}): SessionSummary {
  return {
    id: 's1',
    cwd: '/Users/ada/code/app',
    title: 'Fix the login bug',
    lastPrompt: 'try again',
    updatedAt: new Date().toISOString(),
    sizeBytes: 100,
    running: false,
    openInDesktop: false,
    inDesktop: false,
    unmovableReason: null,
    ...overrides,
  }
}

function plan(overrides: Partial<TransferPlan> = {}): TransferPlan {
  return {
    sessionId: 's1',
    title: 'Fix the login bug',
    cwd: '/Users/ada/code/app',
    sourceLabel: 'Work',
    destinationLabel: 'Personal',
    items: [
      { path: 'projects/-code/s1', action: 'copy' },
      { path: 'projects/-code/s1.jsonl', action: 'copy' },
    ],
    destinationNewer: false,
    desktop: 'add',
    desktopReason: null,
    blockers: [],
    appsToQuit: [],
    notes: ["Connectors come from Personal's own settings."],
    ...overrides,
  }
}

beforeEach(() => {
  vi.mocked(listProfiles).mockResolvedValue([
    profile('work', 'Work'),
    profile('personal', 'Personal'),
    profile('gpt', 'GPT', 'codex'),
  ])
  vi.mocked(listSessions).mockReset()
  vi.mocked(planSessionTransfer).mockReset()
  vi.mocked(transferSession).mockReset()
  vi.mocked(archiveSession).mockReset()
  vi.mocked(checkSessionArchive).mockReset()
})

async function openMoveDialog() {
  const user = userEvent.setup()
  await user.click(await screen.findByRole('button', { name: 'Move' }))
  return { user, dialog: await screen.findByRole('dialog') }
}

describe('ProfileDetailSessions', () => {
  it('lists sessions by title and folder, and says why some cannot move', async () => {
    vi.mocked(listSessions).mockResolvedValue([
      session(),
      session({ id: 's2', title: null, lastPrompt: 'what now', running: true }),
      session({ id: 's3', title: 'Scratch', unmovableReason: 'It works in a scratch folder.' }),
      session({ id: 's4', title: 'Desktop one', running: true, openInDesktop: true }),
    ])
    renderWithQuery(<ProfileDetailSessions profileId="work" />)

    expect(await screen.findByText('Fix the login bug')).toBeInTheDocument()
    expect(screen.getAllByText(/~\/code\/app/)).toHaveLength(4)
    expect(screen.getByText('what now')).toBeInTheDocument()
    expect(screen.getAllByText('Open')).toHaveLength(2)
    expect(screen.getAllByText('Close to move or archive')).toHaveLength(1)
    expect(screen.getByText("Can't move")).toHaveAttribute('title', 'It works in a scratch folder.')
    const pill = (name: string) =>
      within(screen.getByText(name).closest('li') as HTMLElement).getByText(/^(Desktop|CLI)$/).textContent
    expect(pill('Fix the login bug')).toBe('CLI')
    expect(pill('Desktop one')).toBe('Desktop')
    expect(screen.getAllByRole('button', { name: 'Move' })).toHaveLength(2)
    expect(listSessions).toHaveBeenCalledWith('work')
  })

  it('offers the other Claude profiles and the stock install, never Codex ones', async () => {
    vi.mocked(listSessions).mockResolvedValue([session()])
    vi.mocked(planSessionTransfer).mockResolvedValue(plan())
    renderWithQuery(<ProfileDetailSessions profileId="work" />)
    const { dialog } = await openMoveDialog()

    const options = within(dialog)
      .getAllByRole('option')
      .map((option) => option.textContent)
    expect(options).toEqual(['Default (stock install)', 'Personal'])
  })

  it('holds the move while something blocks it', async () => {
    vi.mocked(listSessions).mockResolvedValue([session()])
    vi.mocked(planSessionTransfer).mockResolvedValue(
      plan({ blockers: ['Quit Claude (Personal) first: it rewrites its session list while it is open.'] }),
    )
    renderWithQuery(<ProfileDetailSessions profileId="work" />)
    const { dialog } = await openMoveDialog()

    expect(await within(dialog).findByText(/Quit Claude \(Personal\) first/)).toBeInTheDocument()
    expect(within(dialog).getByRole('button', { name: /^Move/ })).toBeDisabled()
  })

  it('moves the session with the chosen options and reports where things went', async () => {
    vi.mocked(listSessions).mockResolvedValue([session()])
    vi.mocked(planSessionTransfer).mockResolvedValue(plan())
    vi.mocked(transferSession).mockResolvedValue({
      destinationTranscript: '/p/personal/cli-config/projects/-code/s1.jsonl',
      backupDir: null,
      desktopRecord: '/p/personal/gui-data/claude-code-sessions/a/o/local_x.json',
      archivedTo:
        '/Users/ada/Library/Application Support/ai-profiles/profiles/work/cli-config/session-transfer-backups/s1/t-archived',
      memoryCopied: [],
      memoryConflicts: ['notes.md'],
    })
    renderWithQuery(<ProfileDetailSessions profileId="work" />)
    const { user, dialog } = await openMoveDialog()

    await user.selectOptions(within(dialog).getByRole('combobox'), 'personal')
    await user.click(within(dialog).getByRole('checkbox', { name: /Take it out of this profile/ }))
    await within(dialog).findByText('Files: 2 to copy.')
    await user.click(within(dialog).getByRole('button', { name: /^Move/ }))

    await waitFor(() =>
      expect(transferSession).toHaveBeenCalledWith({
        sourceId: 'work',
        sessionId: 's1',
        destinationId: 'personal',
        addToDesktop: true,
        archiveSource: false,
        replaceNewer: false,
        quitApps: false,
      }),
    )
    const done = await screen.findByRole('dialog', { name: 'Session moved' })
    expect(within(done).getByText(/and its desktop app lists it/)).toBeInTheDocument()
    expect(within(done).getByText(/notes\.md/)).toBeInTheDocument()
  })

  it('offers to quit the apps a move needs closed, and asks the backend to', async () => {
    vi.mocked(listSessions).mockResolvedValue([session()])
    vi.mocked(planSessionTransfer).mockResolvedValue(
      plan({ appsToQuit: [{ profileId: 'personal', label: 'Personal' }] }),
    )
    vi.mocked(transferSession).mockResolvedValue({
      destinationTranscript: '/t',
      backupDir: null,
      desktopRecord: '/r',
      archivedTo: null,
      memoryCopied: [],
      memoryConflicts: [],
    })
    renderWithQuery(<ProfileDetailSessions profileId="work" />)
    const { user, dialog } = await openMoveDialog()

    expect(await within(dialog).findByText(/Claude \(Personal\) will quit first/)).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: /^Quit Claude \(Personal\) and move/ }))
    await waitFor(() => expect(transferSession).toHaveBeenCalledWith(expect.objectContaining({ quitApps: true })))
  })

  it('asks before rolling back a newer copy in the destination', async () => {
    vi.mocked(listSessions).mockResolvedValue([session()])
    vi.mocked(planSessionTransfer).mockResolvedValue(
      plan({ destinationNewer: true, items: [{ path: 'projects/-code/s1.jsonl', action: 'replace' }] }),
    )
    renderWithQuery(<ProfileDetailSessions profileId="work" />)
    const { user, dialog } = await openMoveDialog()

    const replace = await within(dialog).findByRole('checkbox', { name: /has a newer copy/ })
    const move = within(dialog).getByRole('button', { name: /^Move/ })
    expect(move).toBeDisabled()
    await user.click(replace)
    expect(move).toBeEnabled()
  })
})

describe('ProfileDetailSessions — archive', () => {
  it('archives a session after confirming, and shows why when it cannot', async () => {
    vi.mocked(listSessions).mockResolvedValue([session({ inDesktop: true })])
    vi.mocked(checkSessionArchive).mockResolvedValue({ blocker: null, appToQuit: null })
    vi.mocked(archiveSession)
      .mockRejectedValueOnce({ kind: 'Validation', message: 'Quit Claude (Work) first.' })
      .mockResolvedValueOnce({ archivedTo: '/x' })
    renderWithQuery(<ProfileDetailSessions profileId="work" />)
    const user = userEvent.setup()

    await user.click(await screen.findByRole('button', { name: 'Archive' }))
    const dialog = await screen.findByRole('dialog', { name: 'Archive session?' })
    expect(within(dialog).getByText(/and its desktop app/)).toBeInTheDocument()

    await user.click(within(dialog).getByRole('button', { name: /^Archive/ }))
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('Quit Claude (Work) first.')

    await user.click(within(dialog).getByRole('button', { name: /^Archive/ }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(archiveSession).toHaveBeenLastCalledWith({ profileId: 'work', sessionId: 's1', quitApp: false })
  })

  it('offers no archive for a session a terminal has open', async () => {
    vi.mocked(listSessions).mockResolvedValue([session({ running: true })])
    renderWithQuery(<ProfileDetailSessions profileId="work" />)
    expect(await screen.findByText('Close to move or archive')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Archive' })).not.toBeInTheDocument()
  })

  it('offers to quit the desktop app that keeps the session, then archives', async () => {
    vi.mocked(listSessions).mockResolvedValue([session({ inDesktop: true, running: true, openInDesktop: true })])
    vi.mocked(checkSessionArchive).mockResolvedValue({
      blocker: null,
      appToQuit: { profileId: 'work', label: 'Work' },
    })
    vi.mocked(archiveSession).mockResolvedValue({ archivedTo: '/x' })
    renderWithQuery(<ProfileDetailSessions profileId="work" />)
    const user = userEvent.setup()

    await user.click(await screen.findByRole('button', { name: 'Archive' }))
    const dialog = await screen.findByRole('dialog', { name: 'Archive session?' })
    expect(await within(dialog).findByText(/Claude \(Work\) will quit first/)).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: /^Quit Claude \(Work\) and archive/ }))

    await waitFor(() =>
      expect(archiveSession).toHaveBeenCalledWith({ profileId: 'work', sessionId: 's1', quitApp: true }),
    )
  })
})
