import type { AppState, Profile, SessionSummary, TransferPlan, TransferReport } from '@/lib/types'

import { act, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'
import {
  archiveSession,
  checkSessionArchive,
  checkSessionRestore,
  deleteArchivedSession,
  listArchivedSessions,
  listProfiles,
  listSessions,
  loadAppState,
  planSessionTransfer,
  restoreSession,
  transferSession,
} from '@/lib/commands'
import { renderWithQuery } from '@/test/render-with-query'

import { ProfileDetailSessions } from './profile-detail-sessions'

/** The backend's progress events: a test sends one with `hear`. */
const progressListeners: Array<(event: { payload: unknown }) => void> = []
function hear(payload: unknown) {
  for (const listener of progressListeners) {
    listener({ payload })
  }
}
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (_event: string, handler: (event: { payload: unknown }) => void) => {
    progressListeners.push(handler)
    return () => {
      progressListeners.splice(progressListeners.indexOf(handler), 1)
    }
  }),
}))

vi.mock('@/lib/commands', async () => {
  const actual = await vi.importActual<typeof import('@/lib/commands')>('@/lib/commands')
  return {
    ...actual,
    listProfiles: vi.fn(),
    loadAppState: vi.fn(),
    listSessions: vi.fn(),
    archiveSession: vi.fn(),
    checkSessionArchive: vi.fn(),
    listArchivedSessions: vi.fn(),
    checkSessionRestore: vi.fn(),
    restoreSession: vi.fn(),
    deleteArchivedSession: vi.fn(),
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
    leftInDefault: false,
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
    sourceBytes: 2048,
    ...overrides,
  }
}

function appState(defaultProfileNames: AppState['defaultProfileNames'] = {}): AppState {
  return {
    welcomeShown: true,
    migrationDismissedAt: null,
    pathBannerDismissedAt: null,
    themeMode: 'system',
    selectedEntryId: null,
    dockIconAcknowledgedAt: null,
    defaultProfileNames,
  }
}

beforeEach(() => {
  vi.mocked(loadAppState).mockResolvedValue(appState())
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
  vi.mocked(listArchivedSessions).mockReset()
  vi.mocked(listArchivedSessions).mockResolvedValue([])
  vi.mocked(checkSessionRestore).mockReset()
  vi.mocked(restoreSession).mockReset()
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

  it('offers the stock install by the name it was given', async () => {
    vi.mocked(loadAppState).mockResolvedValue(appState({ claude: 'Home' }))
    vi.mocked(listSessions).mockResolvedValue([session()])
    vi.mocked(planSessionTransfer).mockResolvedValue(plan())
    renderWithQuery(<ProfileDetailSessions profileId="work" />)
    const { dialog } = await openMoveDialog()

    const options = within(dialog)
      .getAllByRole('option')
      .map((option) => option.textContent)
    expect(options).toEqual(['Home', 'Personal'])
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
      freedBytes: null,
      deleteError: null,
      memoryCopied: [],
      memoryConflicts: ['notes.md'],
    })
    renderWithQuery(<ProfileDetailSessions profileId="work" />)
    const { user, dialog } = await openMoveDialog()

    await user.selectOptions(within(dialog).getByRole('combobox'), 'personal')
    const afterwards = within(dialog).getByRole('group', { name: /The session on Work/ })
    // Archiving stays the default: it can be undone, and says what it keeps.
    expect(await within(afterwards).findByRole('radio', { name: /Archive it \(2.0 KB\)/ })).toBeChecked()
    expect(within(dialog).queryByRole('checkbox', { name: /desktop app/ })).toBeNull()
    expect(await within(dialog).findByText("Personal's desktop app will list it too.")).toBeInTheDocument()
    await user.click(within(afterwards).getByRole('radio', { name: /Keep it/ }))
    // Keeping both forks the session, and moving it back doesn't merge them.
    expect(within(afterwards).getByRole('note')).toHaveTextContent(/forks the session/)
    await within(dialog).findByText('Files: 2 to copy.')
    await user.click(within(dialog).getByRole('button', { name: /^Move/ }))

    await waitFor(() =>
      expect(transferSession).toHaveBeenCalledWith({
        sourceId: 'work',
        sessionId: 's1',
        destinationId: 'personal',
        addToDesktop: true,
        archiveSource: false,
        deleteSource: false,
        replaceNewer: false,
        quitApps: false,
      }),
    )
    const done = await screen.findByRole('dialog', { name: 'Session moved' })
    expect(within(done).getByText(/and its desktop app lists it/)).toBeInTheDocument()
    expect(within(done).getByText(/notes\.md/)).toBeInTheDocument()
  })

  it('shows each step of the move as it runs', async () => {
    vi.mocked(listSessions).mockResolvedValue([session()])
    vi.mocked(planSessionTransfer).mockResolvedValue(plan())
    let finish: (report: TransferReport) => void = () => {}
    vi.mocked(transferSession).mockReturnValue(
      new Promise((resolve) => {
        finish = resolve
      }),
    )
    renderWithQuery(<ProfileDetailSessions profileId="work" />)
    const { user, dialog } = await openMoveDialog()
    await user.selectOptions(within(dialog).getByRole('combobox'), 'personal')
    await within(dialog).findByText('Files: 2 to copy.')
    await user.click(within(dialog).getByRole('button', { name: /^Move/ }))

    const progress = await within(dialog).findByRole('status', { name: 'Move progress' })
    act(() =>
      hear({
        sessionId: 's1',
        steps: ['Copying it to Personal', "Adding it to Personal's desktop app", 'Archiving the copy in Work'],
        current: 1,
      }),
    )
    expect(await within(progress).findByText("Adding it to Personal's desktop app")).toHaveAttribute(
      'aria-current',
      'step',
    )
    expect(within(progress).getByLabelText('Done')).toBeInTheDocument()
    // Another session's move isn't this one's.
    act(() => hear({ sessionId: 'other', steps: ['Copying it to Personal'], current: 0 }))
    expect(within(progress).getByText('Archiving the copy in Work')).toBeInTheDocument()

    finish({
      destinationTranscript: '/t',
      backupDir: null,
      desktopRecord: '/r',
      archivedTo: '/a',
      freedBytes: null,
      deleteError: null,
      memoryCopied: [],
      memoryConflicts: [],
    })
    expect(await screen.findByRole('dialog', { name: 'Session moved' })).toBeInTheDocument()
  })

  it('deletes the original once moved when asked to, saying what that freed', async () => {
    vi.mocked(listSessions).mockResolvedValue([session()])
    vi.mocked(planSessionTransfer).mockResolvedValue(plan({ sourceBytes: 174_063_616 }))
    vi.mocked(transferSession).mockResolvedValue({
      destinationTranscript: '/p/personal/cli-config/projects/-code/s1.jsonl',
      backupDir: null,
      desktopRecord: '/r',
      archivedTo: null,
      freedBytes: 174_063_616,
      deleteError: null,
      memoryCopied: [],
      memoryConflicts: [],
    })
    renderWithQuery(<ProfileDetailSessions profileId="work" />)
    const { user, dialog } = await openMoveDialog()

    await user.selectOptions(within(dialog).getByRole('combobox'), 'personal')
    const afterwards = within(dialog).getByRole('group', { name: /The session on Work/ })
    await user.click(await within(afterwards).findByRole('radio', { name: /^Delete it/ }))
    expect(
      within(afterwards).getByText(/other sessions in Work may use them, and Personal has its own copies now/),
    ).toBeInTheDocument()
    await within(dialog).findByText('Files: 2 to copy.')
    await user.click(within(dialog).getByRole('button', { name: /^Move/ }))

    await waitFor(() =>
      expect(transferSession).toHaveBeenCalledWith(
        expect.objectContaining({ archiveSource: false, deleteSource: true }),
      ),
    )
    const done = await screen.findByRole('dialog', { name: 'Session moved' })
    expect(within(done).getByText('The original was deleted.')).toBeInTheDocument()
  })

  it('brings a session left in the Default folder into the profile, with nothing to choose', async () => {
    vi.mocked(listSessions).mockResolvedValue([session({ inDesktop: true, leftInDefault: true })])
    vi.mocked(planSessionTransfer).mockResolvedValue(
      plan({ sourceLabel: 'Default', destinationLabel: 'Work', desktop: 'alreadyListed' }),
    )
    vi.mocked(transferSession).mockResolvedValue({
      destinationTranscript: '/p/work/cli-config/projects/-code/s1.jsonl',
      backupDir: null,
      desktopRecord: null,
      archivedTo: '/Users/ada/.claude/session-transfer-backups/s1/t-archived',
      freedBytes: null,
      deleteError: null,
      memoryCopied: [],
      memoryConflicts: [],
    })
    renderWithQuery(<ProfileDetailSessions profileId="work" />)

    expect(await screen.findByText('In Default')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Archive' })).toBeNull()
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: 'Move here' }))
    const dialog = await screen.findByRole('dialog', { name: 'Move session here' })
    expect(within(dialog).queryByRole('combobox')).toBeNull()
    expect(within(dialog).queryByRole('checkbox')).toBeNull()
    await within(dialog).findByText('Files: 2 to copy.')
    await user.click(within(dialog).getByRole('button', { name: /^Move/ }))

    await waitFor(() =>
      expect(transferSession).toHaveBeenCalledWith({
        sourceId: 'default:claude',
        sessionId: 's1',
        destinationId: 'work',
        addToDesktop: true,
        archiveSource: true,
        deleteSource: false,
        replaceNewer: false,
        quitApps: false,
      }),
    )
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
      freedBytes: null,
      deleteError: null,
      memoryCopied: [],
      memoryConflicts: [],
    })
    renderWithQuery(<ProfileDetailSessions profileId="work" />)
    const { user, dialog } = await openMoveDialog()

    expect(await within(dialog).findByText(/Claude \(Personal\) will quit first/)).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: /^Quit and move/ }))
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
    await user.click(within(dialog).getByRole('button', { name: /^Quit and archive/ }))

    await waitFor(() =>
      expect(archiveSession).toHaveBeenCalledWith({ profileId: 'work', sessionId: 's1', quitApp: true }),
    )
  })
})

describe('ProfileDetailSessions — restore', () => {
  const archivedOne = {
    id: 'old',
    archive: '20260101-120000-archived',
    archivedAt: '2026-01-01T12:00:00+01:00',
    title: 'Old work',
    cwd: '/Users/ada/code/old',
    inDesktop: true,
    sizeBytes: 3 * 1024 * 1024,
  }

  it('hides the archived list until asked, then restores after quitting the app', async () => {
    vi.mocked(listSessions).mockResolvedValue([session()])
    vi.mocked(listArchivedSessions).mockResolvedValue([archivedOne])
    vi.mocked(checkSessionRestore).mockResolvedValue({
      blocker: null,
      appToQuit: { profileId: 'work', label: 'Work' },
    })
    vi.mocked(restoreSession).mockResolvedValue({ transcript: '/t' })
    renderWithQuery(<ProfileDetailSessions profileId="work" />)
    const user = userEvent.setup()

    await user.click(await screen.findByRole('button', { name: 'Archived 1 · 3.0 MB' }))
    const list = screen.getByRole('list', { name: 'Archived sessions' })
    expect(within(list).getByText('Old work')).toBeInTheDocument()
    expect(within(list).getByText(/archived .* ago/)).toBeInTheDocument()
    expect(within(list).getByText('3.0 MB')).toBeInTheDocument()

    await user.click(within(list).getByRole('button', { name: 'Restore' }))
    const dialog = await screen.findByRole('dialog', { name: 'Restore session?' })
    expect(await within(dialog).findByText(/Claude \(Work\) will quit first/)).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: /^Quit and restore/ }))

    await waitFor(() =>
      expect(restoreSession).toHaveBeenCalledWith({
        profileId: 'work',
        sessionId: 'old',
        archive: '20260101-120000-archived',
        quitApp: true,
      }),
    )
  })

  it('deletes an archive for good once asked, saying what that frees', async () => {
    vi.mocked(listSessions).mockResolvedValue([])
    vi.mocked(listArchivedSessions).mockResolvedValue([archivedOne])
    vi.mocked(deleteArchivedSession).mockResolvedValue(3 * 1024 * 1024)
    renderWithQuery(
      <ToastProvider>
        <ProfileDetailSessions profileId="work" />
      </ToastProvider>,
    )
    const user = userEvent.setup()

    await user.click(await screen.findByRole('button', { name: 'Archived 1 · 3.0 MB' }))
    await user.click(
      within(screen.getByRole('list', { name: 'Archived sessions' })).getByRole('button', { name: 'Delete' }),
    )
    const dialog = await screen.findByRole('dialog', { name: 'Delete this archive?' })
    expect(within(dialog).getByText(/can't be restored/)).toBeInTheDocument()
    expect(deleteArchivedSession).not.toHaveBeenCalled()
    await user.click(within(dialog).getByRole('button', { name: /Delete, freeing 3.0 MB/ }))

    await waitFor(() =>
      expect(deleteArchivedSession).toHaveBeenCalledWith({
        profileId: 'work',
        sessionId: 'old',
        archive: '20260101-120000-archived',
      }),
    )
    expect((await screen.findAllByText('Deleted, freeing 3.0 MB')).length).toBeGreaterThan(0)
  })

  it('holds a restore while a live copy is back in place', async () => {
    vi.mocked(listSessions).mockResolvedValue([])
    vi.mocked(listArchivedSessions).mockResolvedValue([archivedOne])
    vi.mocked(checkSessionRestore).mockResolvedValue({
      blocker: 'Work already has this session. Archive or move that copy first.',
      appToQuit: null,
    })
    renderWithQuery(<ProfileDetailSessions profileId="work" />)
    const user = userEvent.setup()

    await user.click(await screen.findByRole('button', { name: 'Archived 1 · 3.0 MB' }))
    await user.click(screen.getByRole('button', { name: 'Restore' }))
    const dialog = await screen.findByRole('dialog', { name: 'Restore session?' })
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('already has this session')
    expect(within(dialog).getByRole('button', { name: /^Restore/ })).toBeDisabled()
  })
})
