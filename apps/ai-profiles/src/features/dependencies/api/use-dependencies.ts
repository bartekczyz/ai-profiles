import type { Dependencies } from '@/lib/types'

import { useQueryClient, useSuspenseQuery } from '@tanstack/react-query'

import { checkDependencies } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

type UseDependenciesResult = {
  deps: Dependencies
  refresh: () => Promise<void>
}

export function useDependencies(): UseDependenciesResult {
  const queryClient = useQueryClient()
  const { data } = useSuspenseQuery({
    queryKey: queryKeys.dependencies,
    queryFn: checkDependencies,
  })

  return {
    deps: data,
    refresh: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.dependencies })
    },
  }
}
