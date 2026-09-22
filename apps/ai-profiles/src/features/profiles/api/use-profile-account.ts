import type { ProfileAccount } from '@/lib/types'

import { useQuery } from '@tanstack/react-query'

import { profileAccount } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

/**
 * The account a profile is signed in under, or `null` when nothing on disk
 * names one (never signed in, or signed out). Read from the files the CLI
 * keeps, so it is re-read when the window regains focus: a sign-in happens in
 * the app, not here.
 */
export function useProfileAccount(id: string): ProfileAccount | null | undefined {
  const { data } = useQuery({
    queryKey: queryKeys.profiles.account(id),
    queryFn: () => profileAccount(id),
    refetchOnWindowFocus: 'always',
  })
  return data
}
