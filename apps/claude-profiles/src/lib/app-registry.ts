/**
 * Per-app registry (TypeScript mirror of `src-tauri/src/app_kind.rs`).
 *
 * Single source of truth for user-facing surface copy, install links, usage
 * capability, and theming tokens of each managed app. UI reads from
 * `appSpecs[appId]` so adding an app is config, not new components.
 */

export type AppId = 'claude' | 'codex'

export type AppSurfaceSpec = {
  label: string
  description: string
  installUrl: string
}

export type AppUsageSpec = {
  /** Copy shown when the profile is not signed in / has no credentials. */
  noCredentials: string
  /** Labels for the meters this app renders, in render order. */
  primaryLabel: string
  primaryShortLabel: string
  secondaryLabel: string
  secondaryShortLabel: string
  /** Third "Sonnet-style" meter — null for apps without one (Codex). */
  secondaryExtraLabel: string | null
  secondaryExtraShortLabel: string | null
}

export type AppSpec = {
  id: AppId
  displayName: string
  hasUsage: boolean
  gui: AppSurfaceSpec
  cli: AppSurfaceSpec
  /** Usage-card copy, present only when `hasUsage` is true. */
  usage: AppUsageSpec | null
  /** CSS custom-property name driving this app's accent. */
  accentVar: string
}

const claude: AppSpec = {
  id: 'claude',
  displayName: 'Claude',
  hasUsage: true,
  gui: {
    label: 'Desktop App launcher',
    description: 'Creates /Applications/Claude (Name).app with an isolated user-data directory.',
    installUrl: 'https://claude.ai/download',
  },
  cli: {
    label: 'Claude Code CLI wrapper',
    description: 'Exposes claude-{slug} in ~/.local/bin, pointed at this profile.',
    installUrl: 'https://docs.anthropic.com/en/docs/claude-code/overview',
  },
  usage: {
    noCredentials: 'Sign in to Claude Code once with this profile to see usage.',
    primaryLabel: '5-hour window',
    primaryShortLabel: '5h',
    secondaryLabel: 'Weekly',
    secondaryShortLabel: 'W',
    secondaryExtraLabel: 'Weekly Sonnet',
    secondaryExtraShortLabel: 'WS',
  },
  accentVar: '--color-orange',
}

const codex: AppSpec = {
  id: 'codex',
  displayName: 'Codex',
  hasUsage: true,
  gui: {
    label: 'Desktop App launcher',
    description: 'Creates /Applications/Codex (Name).app with an isolated user-data directory.',
    installUrl: 'https://chatgpt.com/codex',
  },
  cli: {
    label: 'Codex CLI wrapper',
    description: 'Exposes codex-{slug} in ~/.local/bin, pointed at this profile (CODEX_HOME).',
    installUrl: 'https://www.npmjs.com/package/@openai/codex',
  },
  usage: {
    noCredentials: 'Sign in to Codex once with this profile to see usage.',
    primaryLabel: '5-hour window',
    primaryShortLabel: '5h',
    secondaryLabel: 'Weekly',
    secondaryShortLabel: 'W',
    secondaryExtraLabel: null,
    secondaryExtraShortLabel: null,
  },
  accentVar: '--color-codex',
}

export const appSpecs: Record<AppId, AppSpec> = { claude, codex }

export const appIds: ReadonlyArray<AppId> = ['claude', 'codex']
