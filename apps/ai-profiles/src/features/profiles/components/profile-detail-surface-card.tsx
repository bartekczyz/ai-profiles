import type { ReactNode } from 'react'
import type { AppId } from '@/lib/app-registry'

import { Monitor, Terminal } from 'lucide-react'

import { cn, Kbd, Skeleton, StatusPulse } from '@/design'
import { appSpecs } from '@/lib/app-registry'

type SurfaceVariant = 'gui' | 'cli'

type SecondaryAction = {
  label: string
  kbd: string
  // Omit `onClick` to render the button disabled (used during the paths
  // fetch: labels + kbd chips stay visible so the layout doesn't shift).
  onClick?: () => void
}

type Props = {
  variant: SurfaceVariant
  app: AppId
  enabled: boolean
  primaryLabel: ReactNode
  primaryKbd: string
  // When `statusDetail` is undefined, the row renders a skeleton bar
  // (used while useProfilePaths resolves on profile switches).
  statusDetail?: string
  statusTone?: 'success' | 'warning'
  secondaries: ReadonlyArray<SecondaryAction>
  primarySuffix?: ReactNode
  onPrimary: () => void
}

const surfaceIcon: Record<SurfaceVariant, ReactNode> = {
  gui: <Monitor className="h-3.5 w-3.5" strokeWidth={1.85} />,
  cli: <Terminal className="h-3.5 w-3.5" strokeWidth={1.85} />,
}

/**
 * Per-app surface-card copy, sourced from the registry so a Codex profile
 * describes Codex surfaces (Codex.app, the `codex` binary, `CODEX_HOME`,
 * `codex-<slug>`) rather than the Claude defaults.
 */
function surfaceCopy(app: AppId, variant: SurfaceVariant): { title: string; description: string } {
  const spec = appSpecs[app]
  if (variant === 'gui') {
    return {
      title: 'Desktop App',
      description: `Launches ${spec.guiBundleName} with an isolated user-data directory so logins, history, and chats stay separate.`,
    }
  }
  return {
    title: `${spec.displayName} CLI`,
    description: `Wraps the ${spec.cliBinary} binary with ${spec.cliConfigEnv} pointed at this profile, exposed as ${spec.cliWrapperPrefix}-{slug}.`,
  }
}

/**
 * Surface card variants share the same shape — icon row, description,
 * status pulse + detail, primary CTA, two secondary buttons — and differ
 * only in the icon, title, and copy carried by `COPY[variant]`. The
 * caller picks the primary CTA label/kbd and provides handlers.
 *
 * Disabled state is styled with reduced opacity and disabled action buttons.
 * Toggling enable/disable happens from the Edit modal (Phase 7); the card
 * is read-only for that decision.
 */
export function ProfileDetailSurfaceCard({
  variant,
  app,
  enabled,
  primaryLabel,
  primaryKbd,
  statusDetail,
  statusTone = 'success',
  secondaries,
  primarySuffix,
  onPrimary,
}: Props) {
  const meta = surfaceCopy(app, variant)
  const icon = surfaceIcon[variant]
  return (
    <section
      data-enabled={enabled ? 'true' : 'false'}
      className="group/card rounded-xl border border-border bg-white p-5 transition-[border-color,box-shadow] duration-(--duration-snap) ease-(--ease-natural) hover:border-border-strong hover:shadow-card-hover data-[enabled=false]:opacity-60 dark:bg-cream-2"
    >
      <header className="mb-2 flex items-center gap-2.5">
        <span
          aria-hidden
          className="grid h-[26px] w-[26px] place-items-center rounded-[7px] bg-cream-2 text-ink-soft dark:bg-cream-3"
        >
          {icon}
        </span>
        <h3 className="text-h3 font-semibold text-ink tracking-[-0.012em]">{meta.title}</h3>
      </header>
      <p className="m-0 mb-3 text-body text-muted leading-[1.5]">{meta.description}</p>
      <div className="mb-4 flex h-[16px] items-center gap-1.5 text-meta font-medium tracking-[-0.003em] text-muted">
        {enabled ? (
          <StatusPulse tone={statusTone} />
        ) : (
          <span aria-hidden className="inline-block h-1.5 w-1.5 rounded-full bg-muted-strong" />
        )}
        {!enabled ? (
          <span>Surface disabled — re-enable from Edit profile</span>
        ) : statusDetail ? (
          <span>{statusDetail}</span>
        ) : (
          <Skeleton shape="text" className="h-2.5 w-48" />
        )}
      </div>
      <div className="flex flex-wrap items-center gap-1.5">
        <button
          type="button"
          onClick={onPrimary}
          disabled={!enabled}
          className="inline-flex h-7 cursor-pointer items-center gap-1.5 rounded-md border-0 bg-[linear-gradient(180deg,var(--color-orange),var(--color-orange-deep))] px-3 text-[12.5px] font-medium text-white shadow-[0_1px_2px_rgba(191,98,64,0.35),inset_0_1px_0_rgba(255,255,255,0.2)] outline-none transition-[filter,transform] duration-(--duration-snap) ease-(--ease-natural) hover:not-disabled:brightness-[0.96] active:not-disabled:translate-y-px disabled:cursor-not-allowed disabled:opacity-60"
        >
          <span>{primaryLabel}</span>
          <Kbd variant="onOrange">{primaryKbd}</Kbd>
        </button>
        {primarySuffix}
        {secondaries.map((action) => (
          <button
            key={action.label}
            type="button"
            onClick={action.onClick}
            disabled={!enabled || action.onClick === undefined}
            className={cn(
              'inline-flex h-7 cursor-pointer items-center gap-1.5 rounded-md border border-border bg-cream px-2.5 text-[12px] font-medium text-ink-soft shadow-[0_1px_0_rgba(40,30,20,0.03)] outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural)',
              'hover:not-disabled:border-border-strong hover:not-disabled:bg-white hover:not-disabled:text-ink',
              'disabled:cursor-not-allowed disabled:opacity-60',
              'dark:bg-white/[0.04] dark:hover:not-disabled:bg-white/[0.07]',
            )}
          >
            <span>{action.label}</span>
            <Kbd variant="subtle">{action.kbd}</Kbd>
          </button>
        ))}
      </div>
    </section>
  )
}

export type { SecondaryAction, SurfaceVariant }
