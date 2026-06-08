import { QueryClient } from '@tanstack/react-query'

/**
 * Desktop-app defaults — window focus is not a network event, so refetch on
 * focus and reconnect are off. Reads stay fresh for 30s; the cache is
 * garbage-collected after 5m of disuse. Mutations don't retry because the
 * user-visible action should fail fast rather than silently retry.
 */
export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 30_000,
        gcTime: 5 * 60_000,
        retry: 1,
        refetchOnWindowFocus: false,
        refetchOnReconnect: false,
      },
      mutations: {
        retry: 0,
      },
    },
  })
}
