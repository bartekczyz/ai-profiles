import { ExternalLink, GitBranch, Globe } from 'lucide-react'

import { Button, Dialog, Kbd, useToast } from '@/design'
import { openExternalUrl } from '@/lib/commands'
import { extractErrorMessage } from '@/lib/extract-error-message'

import { useAppMetadata } from '../api/use-app-metadata'

type Props = {
  open: boolean
  onClose: () => void
}

/**
 * About dialog. Pulls package metadata from `Cargo.toml` via the
 * `get_app_metadata` Tauri command — when the user fills in the
 * `repository = …` field in Cargo.toml, the GitHub link auto-appears on
 * next build. Same story for `homepage`, `license`, and `authors`.
 *
 * Author lines parse `Name <email>` Cargo syntax to extract the display
 * name; the email is intentionally not rendered. Repository / homepage
 * open in the user's default browser via `open_external_url`
 * (scheme-whitelisted on the Rust side).
 */
export function AboutDialog({ open, onClose }: Props) {
  const metadata = useAppMetadata()

  const authors = metadata.authors.map(parseAuthor)

  return (
    <Dialog
      open={open}
      title={metadata.name}
      description={metadata.description}
      onClose={onClose}
      foot={
        <Button variant="primary" size="sm" trailingKbd={<Kbd variant="onOrange">⎋</Kbd>} onClick={onClose}>
          Close
        </Button>
      }
    >
      <dl className="grid grid-cols-[80px_1fr] items-center gap-x-4 gap-y-2.5 text-[13px] tracking-[-0.003em]">
        <Field label="Version">
          <span className="font-mono text-muted-strong" data-selectable="true">
            v{metadata.version}
          </span>
        </Field>
        {metadata.license ? (
          <Field label="License">
            <span className="text-ink-soft">{metadata.license}</span>
          </Field>
        ) : null}
        {authors.length > 0 ? (
          <Field label={authors.length === 1 ? 'Author' : 'Authors'}>
            <span className="text-ink">{authors.map((author) => author.name).join(', ')}</span>
          </Field>
        ) : null}
        {metadata.repository ? (
          <Field label="Source">
            <ExternalLinkRow
              icon={<GitBranch className="h-3.5 w-3.5" strokeWidth={1.85} />}
              url={metadata.repository}
            />
          </Field>
        ) : null}
        {metadata.homepage ? (
          <Field label="Website">
            <ExternalLinkRow icon={<Globe className="h-3.5 w-3.5" strokeWidth={1.85} />} url={metadata.homepage} />
          </Field>
        ) : null}
      </dl>
      <p className="mt-5 border-t border-border-soft pt-4 text-[11.5px] text-muted-strong">
        Not affiliated with Anthropic. Claude and Claude.app are trademarks of Anthropic, PBC.
      </p>
    </Dialog>
  )
}

type FieldProps = {
  label: string
  children: React.ReactNode
}

function Field({ label, children }: FieldProps) {
  return (
    <>
      <dt className="font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-muted-strong">{label}</dt>
      <dd className="m-0">{children}</dd>
    </>
  )
}

type ExternalLinkRowProps = {
  icon: React.ReactNode
  url: string
}

/**
 * Source / Website value row.
 *
 * The button has no padding of its own — it sits flush at the left of
 * the value column so its leading icon aligns with the bare text of
 * the rows above (Version, License, Author). Hover affordance is only
 * the colour change on the URL text and trailing arrow; no background
 * pill that would visually shift the row.
 */
function ExternalLinkRow({ icon, url }: ExternalLinkRowProps) {
  const toast = useToast()
  async function handleClick() {
    try {
      await openExternalUrl(url)
    } catch (caught) {
      toast.error('Could not open link.', extractErrorMessage(caught))
    }
  }
  return (
    <button
      type="button"
      onClick={() => void handleClick()}
      className="group inline-flex cursor-pointer items-center gap-2 border-0 bg-transparent p-0 text-left text-ink-soft outline-none"
    >
      <span aria-hidden className="text-muted-strong transition-colors group-hover:text-ink">
        {icon}
      </span>
      <span className="truncate font-mono text-[12px] transition-colors group-hover:text-ink" data-selectable="true">
        {prettifyUrl(url)}
      </span>
      <ExternalLink
        aria-hidden
        className="h-3 w-3 text-muted-strong transition-colors group-hover:text-ink"
        strokeWidth={1.85}
      />
    </button>
  )
}

/**
 * Render the host of a URL as the label, hiding the protocol, path, and
 * query string. So:
 *   https://czyz.it?utm_source=claude-profiles → czyz.it
 *   https://github.com/bartekczyz/claude-profiles → github.com
 * The underlying click handler still opens the full URL — only the
 * display string is trimmed.
 */
function prettifyUrl(url: string): string {
  try {
    return new URL(url).hostname
  } catch {
    return url.replace(/^https?:\/\//, '').replace(/\/$/, '')
  }
}

type ParsedAuthor = { raw: string; name: string }

/** Parse Cargo's `Name <email>` author syntax and discard the email. */
function parseAuthor(raw: string): ParsedAuthor {
  const match = raw.match(/^(.+?)\s*<[^>]+>\s*$/)
  if (match) {
    return { raw, name: match[1].trim() }
  }
  return { raw, name: raw.trim() }
}
