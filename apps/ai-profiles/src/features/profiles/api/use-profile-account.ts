import type { AccountStatus } from '@/lib/types'

import { useQuery } from '@tanstack/react-query'

import { profileAccount } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

/**
 * Whether a profile is signed in, and as whom; `undefined` while it's read.
 * Read from the files its apps keep, so it is re-read when the window
 * regains focus: a sign-in happens in the app, not here.
 */
export function useProfileAccount(id: string): AccountStatus | undefined {
  const { data } = useQuery({
    queryKey: queryKeys.profiles.account(id),
    queryFn: () => profileAccount(id),
    refetchOnWindowFocus: 'always',
  })
  return data
}
