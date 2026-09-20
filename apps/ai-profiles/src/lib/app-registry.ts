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
  /** Copy when the access token needs refreshing (re-run the CLI once). */
  unauthorized: string
  /** Copy when the upstream service rate-limited the usage request. */
  rateLimited: string
  /** Copy when the usage request couldn't reach its data source. */
  networkError: string
  /** Labels for the meters this app renders, in render order. */
  primaryLabel: string
  primaryShortLabel: string
  secondaryLabel: string
  secondaryShortLabel: string
  /** Third "Sonnet-style" meter — null for apps without one (ChatGPT). */
  secondaryExtraLabel: string | null
  secondaryExtraShortLabel: string | null
}

export type AppDockIconSpec = {
  /**
   * Whether a new profile of this app starts with its own Dock icon switched on,
   * once the user has acknowledged what that involves.
   */
  defaultOn: boolean
  /**
   * What the user gives up for it, as a sentence, or null when there is nothing
   * they would notice.
   */
  cost: string | null
}

export type AppSpec = {
  id: AppId
  /**
   * Product name for the GUI/profile-kind identity — profile list,
   * create-profile dialog, "Open X" buttons.
   */
  displayName: string
  /**
   * Product name for the CLI specifically. Diverges from `displayName` only
   * for Codex, whose desktop app was folded into ChatGPT while the CLI kept
   * its own identity.
   */
  cliDisplayName: string
  hasUsage: boolean
  gui: AppSurfaceSpec
  cli: AppSurfaceSpec
  /**
   * What giving a profile of this app its own Dock icon means for it.
   */
  dockIcon: AppDockIconSpec
  /** Usage-card copy, present only when `hasUsage` is true. */
  usage: AppUsageSpec | null
  /** CSS custom-property name driving this app's accent. */
  accentVar: string
  // Per-app launch/wrapper identifiers, mirroring the Rust `AppSpec`
  // (`src-tauri/src/app_kind.rs`). Values MUST match the `CLAUDE`/`CODEX`
  // consts there. UI copy (surface cards, migration dialog, delete dialog,
  // command palette) reads these instead of hardcoding "claude".
  /** Stock GUI bundle name under `/Applications`, e.g. `"Claude.app"`. */
  guiBundleName: string
  /** Real CLI binary the wrapper execs, e.g. `"claude"`. */
  cliBinary: string
  /** Prefix for generated CLI wrappers: `"<prefix>-<slug>"`. */
  cliWrapperPrefix: string
  /** Env var the wrapper exports to point the CLI at the per-profile config dir. */
  cliConfigEnv: string
  /** Stock CLI config directory under `$HOME`, e.g. `"~/.claude"`. */
  cliStockConfigDir: string
  /** Stock GUI support dir under `~/Library/Application Support`. */
  guiStockSupportDir: string
}

const claude: AppSpec = {
  id: 'claude',
  displayName: 'Claude',
  cliDisplayName: 'Claude',
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
  dockIcon: { defaultOn: true, cost: null },
  usage: {
    noCredentials: 'Sign in to Claude Code once with this profile to see usage.',
    unauthorized: 'Token refresh needed — run `claude` in a terminal once, then retry.',
    rateLimited: 'Rate limited by Anthropic. Try again in a few minutes.',
    networkError: "Couldn't reach Anthropic — check your connection and retry.",
    primaryLabel: '5-hour window',
    primaryShortLabel: '5h',
    secondaryLabel: 'Weekly',
    secondaryShortLabel: 'W',
    secondaryExtraLabel: 'Weekly Sonnet',
    secondaryExtraShortLabel: 'WS',
  },
  accentVar: '--color-orange',
  guiBundleName: 'Claude.app',
  cliBinary: 'claude',
  cliWrapperPrefix: 'claude',
  cliConfigEnv: 'CLAUDE_CONFIG_DIR',
  cliStockConfigDir: '~/.claude',
  guiStockSupportDir: '~/Library/Application Support/Claude',
}

const codex: AppSpec = {
  id: 'codex',
  // OpenAI folded the standalone Codex desktop app into ChatGPT (same
  // bundle id, renamed bundle/executable — no separate Codex app ships any
  // more). The CLI is unaffected and still called Codex, hence the
  // displayName / cliDisplayName split.
  displayName: 'ChatGPT',
  cliDisplayName: 'Codex',
  hasUsage: true,
  gui: {
    label: 'Desktop App launcher',
    description: 'Creates /Applications/ChatGPT (Name).app with an isolated user-data directory.',
    installUrl: 'https://chatgpt.com/codex',
  },
  cli: {
    label: 'Codex CLI wrapper',
    description: 'Exposes codex-{slug} in ~/.local/bin, pointed at this profile (CODEX_HOME).',
    installUrl: 'https://www.npmjs.com/package/@openai/codex',
  },
  // ChatGPT is signed with a push-notification entitlement (`aps-environment`)
  // that belongs to OpenAI's team, so the wrapper has to drop it.
  dockIcon: { defaultOn: false, cost: "Notifications don't work for this profile." },
  usage: {
    noCredentials: 'Sign in to ChatGPT once with this profile to see usage.',
    unauthorized: 'Token refresh needed — run `codex` in a terminal once, then retry.',
    rateLimited: 'Rate limited by OpenAI. Try again in a few minutes.',
    networkError: "Couldn't read ChatGPT usage — make sure the Codex CLI is installed and signed in.",
    primaryLabel: '5-hour window',
    primaryShortLabel: '5h',
    secondaryLabel: 'Weekly',
    secondaryShortLabel: 'W',
    secondaryExtraLabel: null,
    secondaryExtraShortLabel: null,
  },
  accentVar: '--color-chatgpt',
  guiBundleName: 'ChatGPT.app',
  cliBinary: 'codex',
  cliWrapperPrefix: 'codex',
  cliConfigEnv: 'CODEX_HOME',
  cliStockConfigDir: '~/.codex',
  guiStockSupportDir: '~/Library/Application Support/Codex',
}

export const appSpecs: Record<AppId, AppSpec> = { claude, codex }

export const appIds: ReadonlyArray<AppId> = ['claude', 'codex']

/**
 * The per-profile CLI command for a managed profile, e.g. `claude-work`
 * for a Claude profile slugged `work`, `codex-work` for a ChatGPT one. Single
 * source of truth for the wrapper command — used by the surface cards,
 * command palette, copy-CLI shortcut, and clipboard copies so the string is
 * derived in one place rather than hardcoded as `claude-<slug>`.
 */
export function wrapperCommand(app: AppId, slug: string): string {
  return `${appSpecs[app].cliWrapperPrefix}-${slug}`
}

/**
 * The wrapper file installed under `~/.local/bin` for a managed profile,
 * e.g. `~/.local/bin/claude-work`. Used by the delete dialog so a ChatGPT
 * profile lists the correct file to remove.
 */
export function wrapperFileName(app: AppId, slug: string): string {
  return `~/.local/bin/${wrapperCommand(app, slug)}`
}
