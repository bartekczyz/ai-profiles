# claude-profiles

> Run multiple Anthropic Claude accounts on one Mac — the desktop app and the Claude Code CLI, side by side. Free and open-source.

![screenshot placeholder](docs/screenshot.png)

## Landing page

The marketing landing page lives in [`apps/landing/`](apps/landing/README.md) (Astro + Tailwind, deployed to Vercel).

## Install

### Download

Download the latest `.dmg` from [Releases](https://github.com/bartekczyz/claude-profiles/releases/latest). Open it, drag `claude-profiles.app` to `/Applications`. Launch.

### Homebrew

```sh
# placeholder — Homebrew cask coming after v0.1.0 ships
brew install --cask claude-profiles
```

### Build from source

```sh
git clone https://github.com/bartekczyz/claude-profiles.git
cd claude-profiles
pnpm install
pnpm --filter claude-profiles tauri build
```

The `.dmg` lands in `apps/claude-profiles/src-tauri/target/release/bundle/dmg/`. macOS will warn on first launch because the local build isn't notarized — right-click the `.app` → Open → Open to bypass Gatekeeper.

## How it works

claude-profiles is a thin orchestrator that uses two well-known tricks:

**For Claude Desktop:** Claude Desktop is an Electron app, so it accepts Chromium's `--user-data-dir` flag. Each profile gets a `.app` launcher in `/Applications/Claude (<Name>).app` whose shell script does:

```sh
exec open -n -a "Claude" --args --user-data-dir="$HOME/Library/Application Support/claude-profiles/profiles/<id>/gui-data"
```

That points Claude at a fresh data directory per profile — separate cookies, login, history, settings.

**For Claude Code CLI:** Claude Code respects `CLAUDE_CONFIG_DIR`, which relocates its config tree. On macOS, credentials live in the Keychain under a service name derived from `SHA-256(CLAUDE_CONFIG_DIR)[:8]`, so setting a different config dir per profile gives each one its own credential entry automatically. Each profile gets a wrapper at `~/.local/bin/claude-<slug>` that exports the env var and execs `claude`.

The app generates both artifacts on profile create and cleans them up on delete.

## FAQ

**Why do I have to log in to Claude Code again after importing my existing install?**
The Keychain service name is derived from your `CLAUDE_CONFIG_DIR`, which changes when you move into a per-profile directory. The credentials don't carry over. You only need to log in once per profile.

**Why does the Dock show Claude's regular icon instead of my profile's color?**
The launcher `.app` immediately execs the real Claude.app, so the running Dock icon belongs to Claude. The custom color shows up in Spotlight, Finder, Launchpad, and Cmd-Tab — anywhere the launcher itself is referenced. We're tracking better Dock-icon options for v1.1.

**My new profile launcher doesn't open as `claude-<slug>` from my terminal.**
`~/.local/bin` isn't on your PATH. Open Settings → System and click "Install / re-install hook", or add the line manually to your `.zshrc` / `.bashrc` / `config.fish`. Open a new terminal to pick up the change.

**Does this send any of my data anywhere?**
No. The app is local-only and has no analytics, no telemetry, no remote logging. Auto-update checks the GitHub Releases manifest and that's the only outbound request.

**Can I migrate later if I skipped the first-run prompt?**
Yes — Settings → Data → "Detect and import…".

**Is this safe? What about my Keychain credentials?**
Each profile gets a separate Keychain entry derived from its config directory. We don't read or copy your credentials; Claude Code handles all of that. The CLI isolation behavior depends on undocumented Claude Code internals (its SHA-256 service-name derivation), and could break in a future Claude Code release. If it does we'll patch.

**Why "Settings" instead of "Preferences"?**
macOS deprecated "Preferences" in favor of "Settings" in Ventura. We follow the current convention.

## Not affiliated with Anthropic

claude-profiles is an independent project. It is not endorsed by, affiliated with, or supported by Anthropic. "Claude" and "Anthropic" are trademarks of Anthropic, PBC.

## Acknowledgements

claude-profiles is inspired by [Multi-Claude](https://multiclaude.app/), a paid macOS app that pioneered the `--user-data-dir` profile-wrapper approach for Claude Desktop. claude-profiles extends the idea to the Claude Code CLI and ships free + open-source.

## License

[MIT](./LICENSE)
