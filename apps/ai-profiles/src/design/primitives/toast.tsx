import type { ReactNode } from 'react'

import { createContext, useCallback, useContext, useMemo, useState } from 'react'

import { AlertCircle, CheckCircle2, Info, X } from 'lucide-react'
import { Toast as ToastPrimitive } from 'radix-ui'

import { cn } from '@/design/lib/cn'

type ToastTone = 'success' | 'error' | 'info'

type ToastAction = {
  label: string
  onClick: () => void
}

type ToastItem = {
  id: string
  tone: ToastTone
  title: string
  description?: string
  /** Auto-dismiss after this many ms. Pass `null` to require explicit dismiss. */
  durationMs: number | null
  action?: ToastAction
}

type ToastContextValue = {
  show: (input: {
    tone?: ToastTone
    title: string
    description?: string
    durationMs?: number | null
    action?: ToastAction
  }) => string
  success: (title: string, description?: string) => string
  error: (title: string, description?: string) => string
  info: (title: string, description?: string) => string
  dismiss: (id: string) => void
}

const ToastContext = createContext<ToastContextValue | null>(null)

const DEFAULT_DURATION_MS = 5000

const toneIcons: Record<ToastTone, ReactNode> = {
  success: <CheckCircle2 className="h-4 w-4 text-green" strokeWidth={2} />,
  error: <AlertCircle className="h-4 w-4 text-red" strokeWidth={2} />,
  info: <Info className="h-4 w-4 text-orange" strokeWidth={2} />,
}

/**
 * App-wide toast host.
 *
 * Sits at the app root; renders a fixed-position viewport in the bottom-right
 * of the window and a context that any component can pull via `useToast()`
 * to push messages.
 *
 * Use this for ephemeral, non-blocking feedback: a profile failed to save,
 * a backup was removed, the path hook was reinstalled. Don't use it for
 * data the user needs to read in detail or interact with (use a Dialog
 * for that).
 */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Array<ToastItem>>([])

  const dismiss = useCallback((id: string) => {
    setToasts((current) => current.filter((toast) => toast.id !== id))
  }, [])

  const show = useCallback<ToastContextValue['show']>(({ tone = 'info', title, description, durationMs, action }) => {
    const id = `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
    setToasts((current) => [
      ...current,
      {
        id,
        tone,
        title,
        description,
        durationMs: durationMs === undefined ? DEFAULT_DURATION_MS : durationMs,
        action,
      },
    ])
    return id
  }, [])

  const value = useMemo<ToastContextValue>(
    () => ({
      show,
      success: (title, description) => show({ tone: 'success', title, description }),
      error: (title, description) => show({ tone: 'error', title, description }),
      info: (title, description) => show({ tone: 'info', title, description }),
      dismiss,
    }),
    [show, dismiss],
  )

  return (
    <ToastContext.Provider value={value}>
      <ToastPrimitive.Provider swipeDirection="right" duration={DEFAULT_DURATION_MS}>
        {children}
        {toasts.map((toast) => (
          <ToastItemRoot key={toast.id} toast={toast} onDismiss={() => dismiss(toast.id)} />
        ))}
        <ToastPrimitive.Viewport className="fixed bottom-4 right-4 z-[60] flex w-[360px] max-w-[calc(100vw-32px)] flex-col gap-2 outline-none" />
      </ToastPrimitive.Provider>
    </ToastContext.Provider>
  )
}

type ToastItemRootProps = {
  toast: ToastItem
  onDismiss: () => void
}

function ToastItemRoot({ toast, onDismiss }: ToastItemRootProps) {
  return (
    <ToastPrimitive.Root
      duration={toast.durationMs ?? Number.POSITIVE_INFINITY}
      onOpenChange={(open) => {
        if (!open) {
          onDismiss()
        }
      }}
      className={cn(
        'flex items-start gap-3 rounded-xl border border-border bg-cream px-3.5 py-3 shadow-card-hover',
        'data-[state=open]:animate-[design-toast-in_0.22s_cubic-bezier(0.16,1,0.3,1)]',
        'data-[state=closed]:animate-[design-toast-out_0.16s_cubic-bezier(0.4,0,1,1)]',
        'data-[swipe=move]:translate-x-[var(--radix-toast-swipe-move-x)]',
        'data-[swipe=cancel]:translate-x-0 data-[swipe=cancel]:transition-transform',
        'data-[swipe=end]:translate-x-[var(--radix-toast-swipe-end-x)]',
        'dark:bg-cream-2',
      )}
    >
      <span aria-hidden className="mt-0.5 shrink-0">
        {toneIcons[toast.tone]}
      </span>
      <div className="min-w-0 flex-1">
        <ToastPrimitive.Title className="text-[13px] font-medium tracking-[-0.005em] text-ink">
          {toast.title}
        </ToastPrimitive.Title>
        {toast.description ? (
          <ToastPrimitive.Description
            data-selectable="true"
            className="mt-0.5 text-[12px] tracking-[-0.003em] text-muted-strong"
          >
            {toast.description}
          </ToastPrimitive.Description>
        ) : null}
        {toast.action ? (
          <ToastPrimitive.Action asChild altText={toast.action.label}>
            <button
              type="button"
              onClick={toast.action.onClick}
              className="mt-2 inline-flex h-7 items-center rounded-md border border-border bg-white px-2.5 text-[11.5px] font-medium tracking-[-0.003em] text-ink outline-none transition-colors hover:bg-cream hover:text-ink focus-visible:ring-2 focus-visible:ring-ring dark:bg-cream-2 dark:hover:bg-white/[0.06]"
            >
              {toast.action.label}
            </button>
          </ToastPrimitive.Action>
        ) : null}
      </div>
      <ToastPrimitive.Close
        aria-label="Dismiss"
        className="grid h-6 w-6 shrink-0 cursor-pointer place-items-center rounded-md text-muted-strong outline-none transition-colors hover:bg-cream-2 hover:text-ink dark:hover:bg-white/[0.06]"
      >
        <X className="h-3.5 w-3.5" strokeWidth={1.85} />
      </ToastPrimitive.Close>
    </ToastPrimitive.Root>
  )
}

export function useToast(): ToastContextValue {
  const context = useContext(ToastContext)
  if (!context) {
    throw new Error('useToast must be used inside a <ToastProvider>')
  }
  return context
}

export type { ToastTone }
