import type { KeyboardEvent, ReactNode } from 'react'

import { Dialog as DialogPrimitive } from 'radix-ui'

import { cn } from '@/design/lib/cn'

type DialogProps = {
  open: boolean
  title: string
  description?: ReactNode
  head?: ReactNode
  foot?: ReactNode
  className?: string
  /**
   * Set when the dialog opens over another one. Its backdrop is darker then, so
   * the dialog underneath recedes instead of competing with it.
   */
  stacked?: boolean
  /**
   * Whether pressing the page behind the dialog closes it. Turn it off for a
   * dialog that holds something the user has been filling in, which a stray
   * press would otherwise throw away; Escape and the dialog's own buttons still
   * close it.
   */
  closeOnOutsideClick?: boolean
  children: ReactNode
  onClose: () => void
  /**
   * Fires when the user presses Enter while focus is anywhere inside the
   * dialog — input, button, swatch, checkbox. Use it to invoke the same
   * handler the primary action button calls; the consumer's button keeps
   * its own onClick so click and Enter share one code path.
   *
   * Skipped when focus is in a `<textarea>` (Enter means newline there) or
   * when the event has already been default-prevented.
   */
  onSubmit?: () => void
}

/**
 * Cream-surfaced dialog matching the prototype: 14px radius, soft border,
 * tokenized animated entrance via the `design-modal-in` keyframe.
 *
 * Slot layout:
 *   ┌─────────────────────────────┐
 *   │ head (auto: title + descr)  │
 *   ├─────────────────────────────┤
 *   │ children (the body)         │
 *   ├─────────────────────────────┤
 *   │ foot (action row + kbds)    │
 *   └─────────────────────────────┘
 *
 * Pass `head` to override the default title block. Pass `foot` to render
 * footer actions (Cancel + primary, etc.) — the dialog itself doesn't
 * impose footer semantics so danger dialogs and confirm dialogs share
 * the same primitive.
 */
export function Dialog({
  open,
  title,
  description,
  head,
  foot,
  className,
  stacked = false,
  closeOnOutsideClick = true,
  children,
  onClose,
  onSubmit,
}: DialogProps) {
  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (!onSubmit || event.key !== 'Enter' || event.isDefaultPrevented()) {
      return
    }
    const target = event.target as HTMLElement | null
    // Enter in a textarea is a newline, not a submit; one that takes its
    // keys itself (a terminal) keeps it too.
    if (target?.tagName === 'TEXTAREA' || target?.closest?.('[data-keeps-escape]')) {
      return
    }
    // preventDefault stops the focused element's default Enter behaviour
    // (e.g. a checkbox button would otherwise toggle), so the dialog gets
    // one canonical "user committed" signal regardless of focus.
    event.preventDefault()
    onSubmit()
  }

  return (
    <DialogPrimitive.Root open={open} onOpenChange={(next) => (next ? null : onClose())}>
      <DialogPrimitive.Portal>
        {/* Overlay and content share a z-index on purpose. Each dialog is portaled
            after the ones already open, so with equal z-indexes everything of a
            newer dialog, backdrop included, paints over everything of an older one. */}
        <DialogPrimitive.Overlay
          className={cn(
            'fixed inset-0 z-50 bg-[rgba(26,24,21,0.18)] backdrop-blur-sm backdrop-saturate-150 data-[state=closed]:opacity-0 dark:bg-[rgba(0,0,0,0.55)]',
            stacked && 'bg-[rgba(26,24,21,0.42)] dark:bg-[rgba(0,0,0,0.72)]',
          )}
        />
        <DialogPrimitive.Content
          onKeyDown={handleKeyDown}
          onEscapeKeyDown={(event) => {
            // An element that takes Escape itself (a terminal) keeps it.
            if ((event.target as HTMLElement | null)?.closest?.('[data-keeps-escape]')) {
              event.preventDefault()
            }
          }}
          onPointerDownOutside={(event) => {
            if (!closeOnOutsideClick) {
              event.preventDefault()
            }
          }}
          className={cn(
            'fixed left-1/2 top-1/2 z-50 -translate-x-1/2 -translate-y-1/2 outline-none',
            'w-[min(440px,calc(100%-64px))] rounded-xl border border-[color:rgba(40,30,20,0.1)] bg-cream',
            'shadow-modal animate-[design-modal-in_0.22s_cubic-bezier(0.16,1,0.3,1)]',
            'flex max-h-[calc(100%-64px)] flex-col overflow-hidden',
            'dark:border-white/[0.06]',
            className,
          )}
        >
          {head ?? (
            <header className="border-b border-border-soft px-6 pt-5 pb-3.5">
              <DialogPrimitive.Title className="m-0 mb-1 font-sans text-h2 font-semibold tracking-[-0.018em] text-ink">
                {title}
              </DialogPrimitive.Title>
              {description ? (
                <DialogPrimitive.Description className="m-0 text-meta text-muted tracking-[-0.003em]">
                  {description}
                </DialogPrimitive.Description>
              ) : null}
            </header>
          )}
          <div className="flex-1 overflow-y-auto px-6 pt-5 pb-6">{children}</div>
          {foot ? (
            <footer className="flex items-center justify-end gap-2 border-t border-border-soft bg-[linear-gradient(180deg,transparent,rgba(0,0,0,0.015))] px-6 py-3.5 dark:bg-[linear-gradient(180deg,transparent,rgba(255,255,255,0.02))]">
              {foot}
            </footer>
          ) : null}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  )
}

export type { DialogProps }
