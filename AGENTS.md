# AGENTS.md

## What this is

ai-profiles — a free, open-source macOS app for running multiple Claude and ChatGPT accounts side by side (desktop apps + CLIs). pnpm + Turborepo monorepo:

- `apps/ai-profiles` — the product. Tauri 2 app: React 19 + Vite + Tailwind v4 frontend, Rust backend.
- `apps/landing` — marketing site. Static Astro + Tailwind v4, deployed to Vercel.
- `packages/design-tokens`, `packages/tsconfig` — shared workspace packages.

## Commands

Scripts live in each `package.json`; these are the ones worth knowing.

```sh
pnpm lint / pnpm typecheck / pnpm test   # whole repo (biome; turbo for the rest)
pnpm fallow audit --base origin/main     # dead code + module boundaries, as pre-push runs it

pnpm --filter ai-profiles tauri dev      # run the desktop app
pnpm --filter ai-profiles exec vitest run src/lib/colors.test.ts   # one test file

# Rust, from repo root
cargo test --manifest-path apps/ai-profiles/src-tauri/Cargo.toml
cargo clippy --manifest-path apps/ai-profiles/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path apps/ai-profiles/src-tauri/Cargo.toml --all
```

Lefthook (`lefthook.yml`) runs biome and rustfmt on pre-commit; typecheck, clippy, cargo test, landing tests and fallow audit on pre-push.

## Architecture

### Frontend ↔ backend bridge
All Tauri IPC goes through typed wrappers in `apps/ai-profiles/src/lib/commands.ts`; `invoke()` is called nowhere else. Rust handlers live in `src-tauri/src/commands.rs`; TS types in `src/lib/types.ts` mirror the Rust structs (serde). Adding a command touches all three. Frontend state is TanStack Query over these commands, persisted to localStorage.

### Rust backend (`apps/ai-profiles/src-tauri/src/`)
- `app_kind.rs` — abstraction over supported apps (Claude vs ChatGPT); per-app paths, env vars, naming.
- `profiles.rs` — profile CRUD; data lives under `~/Library/Application Support/ai-profiles/profiles/<id>/`.
- `launchers/` — generates per-profile `.app` bundles (tinted icons, plist) and CLI wrapper scripts in `~/.local/bin` (`claude-<slug>` sets `CLAUDE_CONFIG_DIR`, `codex-<slug>` sets `CODEX_HOME`).
- `migration.rs` — imports an existing `~/.claude` / `~/.codex` install into a profile; originals move to a timestamped backup dir (auto-deleted after 7 days).
- `sessions/` — each profile's Claude Code and Codex sessions: list, archive, restore, move between profiles, repair. Reads the apps' own files, which are undocumented internals: parse leniently and skip what doesn't parse.
- `usage/` — quota meters and API-equivalent cost.
  - Claude quota: `api.anthropic.com/api/oauth/usage` with the OAuth token from the macOS Keychain (service name derived from `CLAUDE_CONFIG_DIR`); `api/oauth/profile` confirms the token belongs to the entry's account. An expired token gets 429, not 401.
  - Token refresh (`refresh.rs`) drives a real `claude` session under a pty so Claude Code rotates its own token; the app never calls Anthropic's token endpoint itself.
  - Codex quota: `codex app-server` JSON-RPC (`account/rateLimits/read`, client in `codex_rpc.rs`).
  - API cost: computed locally from session transcripts × LiteLLM's price list (refreshed daily, falls back to the bundled `price-snapshot.json`).
  - All of these are undocumented internals; degrade to `—` rather than crash.
- `path_setup.rs` — installs the `~/.local/bin` PATH hook into the user's shell rc.

### Frontend (`apps/ai-profiles/src/`)
- `features/<name>/` — one folder per feature.
- `design/` — closed design system, kept movable to `packages/` as-is: it imports nothing from the app (fallow boundary zones) and nothing from `@tauri-apps/*` (`design/boundary.test.ts`). shadcn primitives go in `design/ui/`: add via `shadcn add`, relocate there and rewrite the `cn` import.
- `lib/app-registry.ts` — frontend counterpart of `app_kind.rs` (per-app metadata).

## Conventions

- **Commits**: conventional commits; release-please builds the changelog from them. `feat` (Added), `fix` (Fixed), `perf`, `refactor` (Changed) and `revert` show up; `chore`, `docs`, `test`, `ci`, `build`, `style` stay hidden. Changes users never see (dev-only tooling, agent docs, CI) take a hidden type. Release-please syncs versions in `package.json`, `tauri.conf.json` and `Cargo.toml`; leave them alone.
- **Lint/format**: Biome is the only JS/TS linter and formatter.
- **TS constants**: module-level `const`s are camelCase (`sixHoursMs`), even next to older SCREAMING_CASE ones.
- **Tests**: cover behaviour (interactions, state transitions, conditional rendering, IPC payloads, error paths). Styling and static copy are checked by eye, not asserted.
- **Network**: local-only by design, no telemetry or analytics. Outbound requests are exactly: the update check (GitHub `latest.json`), the Claude usage and profile endpoints, and the LiteLLM price list (carries no user data). A new outbound request is a product decision: ask first.

## Screenshots

`apps/landing/public/screenshot-{light,dark}.png` (README and landing page) are taken from the app itself, on fake data: run `pnpm --filter ai-profiles tauri dev`, then Developer → Capture Screenshots (debug builds only). The app that launches it (terminal or IDE) needs Screen Recording permission (System Settings → Privacy & Security), and only gets it after a restart. What they show lives in `src/features/demo-mode/fixtures.ts`; in demo mode an app command without a handler there is refused, so a new command needs one. Review the PNGs, then commit them.
