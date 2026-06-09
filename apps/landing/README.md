# apps/landing

Marketing landing page for ai-profiles. Static Astro + Tailwind v4.

## Scripts

- `pnpm --filter landing dev` — local dev server on http://localhost:4321
- `pnpm --filter landing build` — emit static site to `dist/`
- `pnpm --filter landing preview` — preview the built site
- `pnpm --filter landing format` — format files with Biome

## Deploying

Hosted on Vercel.

### Vercel project setup (one-time)

1. New Project → Import from GitHub → select `bartekczyz/ai-profiles`.
2. **Root Directory**: `apps/landing`.
3. **Framework Preset**: leave on auto (Vercel detects Astro).
4. **Build Command**: pre-set by `vercel.json` to `pnpm --filter landing build`.
5. **Output Directory**: pre-set by `vercel.json` to `dist`.
6. **Install Command**: pre-set by `vercel.json` to `pnpm install --frozen-lockfile`.
7. **Environment Variables**:
   - `PUBLIC_POSTHOG_KEY` — PostHog project API key
   - `PUBLIC_POSTHOG_HOST` — defaults to `https://eu.i.posthog.com` if unset
8. Click Deploy.

PR previews are automatic for any PR that touches `apps/landing/**` or `packages/design-tokens/**`.
