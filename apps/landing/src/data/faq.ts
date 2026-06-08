export interface FaqEntry {
  question: string
  answer: string
}

export const faqEntries: ReadonlyArray<FaqEntry> = [
  {
    question: 'Does this send any of my data anywhere?',
    answer:
      'No. The app is local-only and has no analytics, no telemetry, no remote logging. The only outbound request it makes is to check the GitHub Releases manifest for updates.',
  },
  {
    question: 'Is it really free?',
    answer:
      'Yes, MIT licensed. There is a paid app called Multi-Claude that pioneered the desktop-app half of this approach — claude-profiles extends it to the Claude Code CLI and ships open-source.',
  },
  {
    question: 'What if I already have Claude or Codex installed?',
    answer:
      'On first launch, claude-profiles asks: (a) keep your existing install separate and start a new profile alongside it (default — press Enter), or (b) migrate your existing install into your first profile. The default leaves your current setup untouched and adds a separate profile reached via claude-<slug> or codex-<slug>. If you choose migrate, the data is copied into the profile dir and the originals are moved to a 7-day backup. You can trigger migration later from Settings → Data → Re-import.',
  },
  {
    question: "What does 'migrate' actually do to my data?",
    answer:
      'Three things, in order: (1) copies the stock app data into the new profile directory under ~/Library/Application Support/claude-profiles/profiles/<id>/ — ~/.claude and ~/Library/Application Support/Claude for a Claude profile, or ~/.codex and ~/Library/Application Support/Codex for a Codex profile; (2) moves the originals into a 7-day backup dir under migration-backup-<timestamp>/; (3) generates the CLI wrapper and launcher (claude-<slug> + Claude (<Name>).app, or codex-<slug> + Codex (<Name>).app). To revert: copy the backup folder contents back to their original locations.',
  },
  {
    question: 'Is it affiliated with Anthropic or OpenAI?',
    answer:
      'No. claude-profiles is an independent project, not affiliated with Anthropic or OpenAI. "Claude" and "Anthropic" are trademarks of Anthropic, PBC. "Codex" and "ChatGPT" are trademarks of OpenAI.',
  },
  {
    question: 'Is it safe? What about my Keychain credentials?',
    answer:
      'Each profile gets its own Keychain entry, derived from the per-profile config directory. claude-profiles does not read or copy your credentials — Claude Code handles all of that itself. The isolation depends on undocumented Claude Code internals and could break in a future Claude Code release; if it does, we will patch.',
  },
  {
    question: 'How does the per-profile usage card work?',
    answer:
      "Each profile's detail page shows that profile's quota utilization and auto-refreshes every 5 minutes. Claude profiles display three meters (5-hour, 7-day, 7-day Sonnet) by reading the profile's OAuth token from its dedicated Keychain entry and calling Anthropic's /api/oauth/usage endpoint. Codex profiles display two meters (5-hour and weekly) by querying the Codex app-server over its JSON-RPC protocol — no extra auth needed, the app-server handles it via the profile's own auth.json. Both quota endpoints are undocumented internals, so the meters may render as dashes if the response shape changes.",
  },
]
