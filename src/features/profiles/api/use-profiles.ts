import type { Profile, ProfilePatch, Surface, Surfaces } from '@/lib/types'

import { useEffect, useState } from 'react'

import { useMutation, useQueryClient, useSuspenseQuery } from '@tanstack/react-query'

import { createProfile, deleteProfile, listProfiles, toggleSurface, updateProfile } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

type UseProfilesResult = {
  profiles: Array<Profile>
  selectedId: string | null
  select: (id: string | null) => void
  create: (input: { name: string; color: string; surfaces: Surfaces }) => Promise<Profile>
  update: (input: { id: string; patch: ProfilePatch }) => Promise<Profile>
  remove: (input: { id: string; moveToTrash: boolean }) => Promise<void>
  toggle: (input: { id: string; surface: Surface; enabled: boolean }) => Promise<Profile>
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
  const [selectedId, setSelectedId] = useState<string | null>(() => profiles[0]?.id ?? null)

  useEffect(() => {
    if (profiles.length === 0) {
      if (selectedId !== null) {
        setSelectedId(null)
      }
      return
    }
    if (selectedId === null || !profiles.some((profile) => profile.id === selectedId)) {
      setSelectedId(profiles[0].id)
    }
  }, [profiles, selectedId])

  const createMutation = useMutation({
    mutationFn: createProfile,
    onSuccess: (created) => {
      queryClient.setQueryData<Array<Profile>>(queryKeys.profiles.all, (previous) =>
        previous ? [...previous, created] : [created],
      )
      setSelectedId(created.id)
    },
  })

  const updateMutation = useMutation({
    mutationFn: updateProfile,
    onSuccess: (updated) => {
      queryClient.setQueryData<Array<Profile>>(queryKeys.profiles.all, (previous) =>
        previous ? setEntry(previous, updated) : [updated],
      )
    },
  })

  const removeMutation = useMutation({
    mutationFn: deleteProfile,
    onSuccess: (_void, variables) => {
      queryClient.setQueryData<Array<Profile>>(queryKeys.profiles.all, (previous) =>
        previous ? previous.filter((profile) => profile.id !== variables.id) : [],
      )
      if (selectedId === variables.id) {
        setSelectedId(null)
      }
    },
  })

  const toggleMutation = useMutation({
    mutationFn: toggleSurface,
    onSuccess: (updated) => {
      queryClient.setQueryData<Array<Profile>>(queryKeys.profiles.all, (previous) =>
        previous ? setEntry(previous, updated) : [updated],
      )
    },
  })

  return {
    profiles,
    selectedId,
    select: setSelectedId,
    create: (input) => createMutation.mutateAsync(input),
    update: (input) => updateMutation.mutateAsync(input),
    remove: (input) => removeMutation.mutateAsync(input).then(() => undefined),
    toggle: (input) => toggleMutation.mutateAsync(input),
    refresh: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.profiles.all })
    },
  }
}
