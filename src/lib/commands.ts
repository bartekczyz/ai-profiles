import type { Profile, Surfaces } from './types'

import { invoke } from '@tauri-apps/api/core'

export function listProfiles(): Promise<Array<Profile>> {
  return invoke<Array<Profile>>('list_profiles')
}

export function createProfile(input: { name: string; color: string; surfaces: Surfaces }): Promise<Profile> {
  return invoke<Profile>('create_profile', input)
}
