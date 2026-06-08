import type { AppMetadata } from '@/lib/types'

import { useSuspenseQuery } from '@tanstack/react-query'

import { getAppMetadata } from '@/lib/commands'

const APP_METADATA_QUERY_KEY = ['app-metadata'] as const

/**
 * Pull the package metadata bundled into the Rust binary (name, version,
 * authors, repository, homepage, license). The values come from Cargo's
 * `env!("CARGO_PKG_*")` macros, so the query is effectively static —
 * `staleTime: Infinity`. Editing `Cargo.toml` and rebuilding is what
 * changes the output.
 */
export function useAppMetadata(): AppMetadata {
  const { data } = useSuspenseQuery({
    queryKey: APP_METADATA_QUERY_KEY,
    queryFn: getAppMetadata,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: Number.POSITIVE_INFINITY,
  })
  return data
}
