import type { ChangelogRelease, ChangelogSection } from '../lib/changelog'

import { Button, Dialog, Kbd } from '@/design'

type WhatsNewDialogProps = {
  /**
   * Whether the dialog is visible.
   */
  open: boolean
  /**
   * Releases to list, newest first. Empty shows a fallback line.
   */
  releases: Array<ChangelogRelease>
  /**
   * Called when the user dismisses the dialog (Close button, Esc, overlay).
   */
  onClose: () => void
}

/**
 * Release notes dialog. Lists each release's version and date, then its sections
 * (Added / Fixed / Changed …) with a muted scope prefix per item. Text is plain — the changelog's
 * markdown is reduced to text by the parser.
 */
export function WhatsNewDialog({ open, releases, onClose }: WhatsNewDialogProps) {
  return (
    <Dialog
      open={open}
      title="What's new"
      onClose={onClose}
      foot={
        <Button variant="primary" size="sm" trailingKbd={<Kbd variant="onOrange">⎋</Kbd>} onClick={onClose}>
          Close
        </Button>
      }
    >
      {releases.length === 0 ? (
        <p className="m-0 text-[13px] text-muted-strong">No release notes available for this version.</p>
      ) : (
        <div className="flex flex-col gap-6">
          {releases.map((release) => (
            <ReleaseNotes key={release.version} release={release} />
          ))}
        </div>
      )}
    </Dialog>
  )
}

type ReleaseNotesProps = {
  /**
   * The release to render.
   */
  release: ChangelogRelease
}

function ReleaseNotes({ release }: ReleaseNotesProps) {
  return (
    <section className="flex flex-col gap-3">
      <h3 className="m-0 flex items-baseline gap-2 text-[13px] font-semibold tracking-[-0.003em] text-ink">
        <span className="font-mono">v{release.version}</span>
        {release.date ? (
          <span className="font-mono text-[11px] font-normal text-muted-strong">{release.date}</span>
        ) : null}
      </h3>
      {release.sections.map((section) => (
        <ReleaseSection key={section.title} section={section} />
      ))}
    </section>
  )
}

type ReleaseSectionProps = {
  /**
   * The changelog section to render.
   */
  section: ChangelogSection
}

function ReleaseSection({ section }: ReleaseSectionProps) {
  return (
    <div className="flex flex-col gap-1.5">
      <h4 className="m-0 font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-muted-strong">
        {section.title}
      </h4>
      <ul className="m-0 flex list-none flex-col gap-1.5 p-0 text-[13px] leading-snug tracking-[-0.003em] text-ink-soft">
        {section.items.map((item) => (
          <li key={`${item.scope ?? ''}:${item.text}`} data-selectable="true">
            {item.scope ? <span className="mr-1.5 font-mono text-[11px] text-muted-strong">{item.scope}</span> : null}
            {item.text}
          </li>
        ))}
      </ul>
    </div>
  )
}
