import { Component, type ErrorInfo, type ReactNode } from 'react'

import { QueryErrorResetBoundary } from '@tanstack/react-query'

import { Button, Card } from '@/design'

type FallbackProps = {
  error: Error
  reset: () => void
}

function DefaultFallback({ error, reset }: FallbackProps) {
  return (
    <div className="flex h-full items-center justify-center p-8">
      <Card className="max-w-md p-5">
        <div className="text-eyebrow uppercase text-muted-strong mb-2">Something went wrong</div>
        <div className="text-body text-ink-soft mb-4">{error.message || 'Unknown error'}</div>
        <Button variant="secondary" size="sm" onClick={reset}>
          Try again
        </Button>
      </Card>
    </div>
  )
}

type BoundaryProps = {
  fallback?: (props: FallbackProps) => ReactNode
  onReset: () => void
  children: ReactNode
}

type BoundaryState = { error: Error | null }

class InnerBoundary extends Component<BoundaryProps, BoundaryState> {
  state: BoundaryState = { error: null }

  static getDerivedStateFromError(error: Error): BoundaryState {
    return { error }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    if (import.meta.env.DEV) {
      console.error('[QueryErrorBoundary]', error, info)
    }
  }

  reset = () => {
    this.setState({ error: null })
    this.props.onReset()
  }

  render() {
    if (this.state.error) {
      const fallback = this.props.fallback ?? DefaultFallback
      return fallback({ error: this.state.error, reset: this.reset })
    }
    return this.props.children
  }
}

type QueryErrorBoundaryProps = {
  fallback?: (props: FallbackProps) => ReactNode
  children: ReactNode
}

/**
 * Wraps a Suspense subtree so IPC failures surface as a tokenized error
 * card. "Try again" both clears the boundary state and resets the
 * TanStack query cache so the failed query refetches.
 */
export function QueryErrorBoundary({ fallback, children }: QueryErrorBoundaryProps) {
  return (
    <QueryErrorResetBoundary>
      {({ reset }) => (
        <InnerBoundary fallback={fallback} onReset={reset}>
          {children}
        </InnerBoundary>
      )}
    </QueryErrorResetBoundary>
  )
}

export type { FallbackProps }
