import type { ReactElement, ReactNode } from 'react'

import { Suspense } from 'react'

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import {
  type RenderHookOptions,
  type RenderHookResult,
  type RenderOptions,
  type RenderResult,
  render,
  renderHook,
} from '@testing-library/react'

/**
 * Each test gets a fresh QueryClient (so cache state never leaks between
 * cases). Mutations and retries are disabled by default so a failing query
 * surfaces immediately as a thrown error rather than being retried.
 */
function makeTestClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        // `Infinity` prevents background refetches from consuming
        // queued `mockResolvedValueOnce` slots intended for mutations.
        staleTime: Number.POSITIVE_INFINITY,
        gcTime: Number.POSITIVE_INFINITY,
        refetchOnWindowFocus: false,
        refetchOnReconnect: false,
        refetchOnMount: false,
      },
      mutations: { retry: 0 },
    },
  })
}

type RenderWithQueryOptions = {
  client?: QueryClient
  suspenseFallback?: ReactNode
} & Omit<RenderOptions, 'wrapper'>

export function renderWithQuery(
  ui: ReactElement,
  options: RenderWithQueryOptions = {},
): RenderResult & { client: QueryClient } {
  const { client = makeTestClient(), suspenseFallback = null, ...rest } = options
  function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={client}>
        <Suspense fallback={suspenseFallback}>{children}</Suspense>
      </QueryClientProvider>
    )
  }
  const result = render(ui, { wrapper: Wrapper, ...rest })
  return { ...result, client }
}

type RenderHookWithQueryOptions<TProps> = {
  client?: QueryClient
  suspenseFallback?: ReactNode
} & Omit<RenderHookOptions<TProps>, 'wrapper'>

export function renderHookWithQuery<TResult, TProps>(
  callback: (props: TProps) => TResult,
  options: RenderHookWithQueryOptions<TProps> = {},
): RenderHookResult<TResult, TProps> & { client: QueryClient } {
  const { client = makeTestClient(), suspenseFallback = null, ...rest } = options
  function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={client}>
        <Suspense fallback={suspenseFallback}>{children}</Suspense>
      </QueryClientProvider>
    )
  }
  const result = renderHook(callback, { wrapper: Wrapper, ...rest })
  return { ...result, client }
}
