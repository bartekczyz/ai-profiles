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
    question: 'What if I already have Claude installed?',
    answer:
      'On first launch, claude-profiles asks: (a) keep your existing Claude separate and start a new profile alongside it (default — press Enter), or (b) migrate your existing install into your first profile. The default leaves your current setup untouched and adds a separate profile reached via claude-<slug>. If you choose migrate, the data is copied into the profile dir and the originals are moved to a 7-day backup. You can trigger migration later from Settings → Data → Re-import.',
  },
  {
    question: "What does 'migrate' actually do to my data?",
    answer:
      'Three things, in order: (1) copies ~/.claude and ~/Library/Application Support/Claude into the new profile directory under ~/Library/Application Support/claude-profiles/profiles/<id>/; (2) moves the originals into a 7-day backup dir under migration-backup-<timestamp>/; (3) generates a claude-<slug> CLI wrapper and a Claude (<Name>).app launcher. To revert: copy the backup folder contents back to their original locations.',
  },
  {
    question: 'Is it affiliated with Anthropic?',
    answer: 'No. claude-profiles is an independent project. "Claude" and "Anthropic" are trademarks of Anthropic, PBC.',
  },
  {
    question: 'Is it safe? What about my Keychain credentials?',
    answer:
      'Each profile gets its own Keychain entry, derived from the per-profile config directory. claude-profiles does not read or copy your credentials — Claude Code handles all of that itself. The isolation depends on undocumented Claude Code internals and could break in a future Claude Code release; if it does, we will patch.',
  },
]
