import type { AppId, Profile, ProfilePatch, Surface, Surfaces } from '@/lib/types'

import { useMutation, useQueryClient, useSuspenseQuery } from '@tanstack/react-query'

import {
  createProfile,
  deleteProfile,
  listProfiles,
  reorderProfiles,
  toggleSurface,
  updateProfile,
} from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

type UseProfilesResult = {
  profiles: Array<Profile>
  create: (input: { app: AppId; name: string; color: string; surfaces: Surfaces }) => Promise<Profile>
  update: (input: { id: string; patch: ProfilePatch }) => Promise<Profile>
  remove: (input: { id: string; moveToTrash: boolean }) => Promise<void>
  toggle: (input: { id: string; surface: Surface; enabled: boolean }) => Promise<Profile>
  reorder: (ids: Array<string>) => Promise<Array<Profile>>
  refresh: () => Promise<void>
}

function setEntry(list: Array<Profile>, updated: Profile): Array<Profile> {
  return list.map((profile) => (profile.id === updated.id ? updated : profile))
}

export function useProfiles(): UseProfilesResult {
  const queryClient = useQueryClient()
  const { data: profiles } = useSuspenseQuery({
    queryKey: queryKeys.profiles.all,
    queryFn: listProfiles,
  })

  const createMutation = useMutation({
    mutationFn: createProfile,
    onSuccess: (created) => {
      queryClient.setQueryData<Array<Profile>>(queryKeys.profiles.all, (previous) =>
        previous ? [...previous, created] : [created],
      )
      void queryClient.invalidateQueries({ queryKey: queryKeys.profiles.activity(created.id) })
    },
  })

  const updateMutation = useMutation({
    mutationFn: updateProfile,
    onSuccess: (updated) => {
      queryClient.setQueryData<Array<Profile>>(queryKeys.profiles.all, (previous) =>
        previous ? setEntry(previous, updated) : [updated],
      )
      void queryClient.invalidateQueries({ queryKey: queryKeys.profiles.activity(updated.id) })
    },
  })

  const removeMutation = useMutation({
    mutationFn: deleteProfile,
    onSuccess: (_void, variables) => {
      queryClient.setQueryData<Array<Profile>>(queryKeys.profiles.all, (previous) =>
        previous ? previous.filter((profile) => profile.id !== variables.id) : [],
      )
    },
  })

  const toggleMutation = useMutation({
    mutationFn: toggleSurface,
    onSuccess: (updated) => {
      queryClient.setQueryData<Array<Profile>>(queryKeys.profiles.all, (previous) =>
        previous ? setEntry(previous, updated) : [updated],
      )
      void queryClient.invalidateQueries({ queryKey: queryKeys.profiles.activity(updated.id) })
    },
  })

  const reorderMutation = useMutation({
    mutationFn: reorderProfiles,
    // Optimistic reorder: paint the new order immediately, roll back on failure.
    onMutate: async (ids) => {
      await queryClient.cancelQueries({ queryKey: queryKeys.profiles.all })
      const previous = queryClient.getQueryData<Array<Profile>>(queryKeys.profiles.all)
      if (previous) {
        const byId = new Map(previous.map((profile) => [profile.id, profile]))
        const optimistic = ids.map((id) => byId.get(id)).filter((profile): profile is Profile => profile !== undefined)
        queryClient.setQueryData(queryKeys.profiles.all, optimistic)
      }
      return { previous }
    },
    onError: (_error, _ids, context) => {
      if (context?.previous) {
        queryClient.setQueryData(queryKeys.profiles.all, context.previous)
      }
    },
    onSuccess: (next) => {
      queryClient.setQueryData(queryKeys.profiles.all, next)
    },
  })

  return {
    profiles,
    create: (input) => createMutation.mutateAsync(input),
    update: (input) => updateMutation.mutateAsync(input),
    remove: (input) => removeMutation.mutateAsync(input).then(() => undefined),
    toggle: (input) => toggleMutation.mutateAsync(input),
    reorder: (ids) => reorderMutation.mutateAsync(ids),
    refresh: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.profiles.all })
    },
  }
}
