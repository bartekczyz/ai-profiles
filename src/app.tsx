import type { AppError, Profile } from './lib/types'

import { useEffect, useState } from 'react'

import { createProfile, listProfiles } from './lib/commands'

export default function App() {
  const [profiles, setProfiles] = useState<Array<Profile>>([])
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    listProfiles()
      .then(setProfiles)
      .catch((caught: AppError) => {
        setError(caught.message)
      })
  }, [])

  async function handleCreate() {
    try {
      const created = await createProfile({
        name: `Test ${new Date().toISOString()}`,
        color: '#7C3AED',
        surfaces: { gui: true, cli: true },
      })
      setProfiles((previous) => [...previous, created])
      setError(null)
    } catch (caught) {
      const cast = caught as AppError
      setError(cast.message)
    }
  }

  return (
    <main className="p-8">
      <h1 className="text-2xl font-semibold mb-4">claude-profiles</h1>
      <p className="text-sm text-[color:var(--color-muted)] mb-6">
        Phase 1 scaffolding screen — Phase 4 replaces this with the real UI.
      </p>
      <button
        type="button"
        onClick={handleCreate}
        className="px-3 py-1.5 rounded-lg border border-[color:var(--color-border)]"
      >
        Create a test profile
      </button>
      {error ? <p className="mt-4 text-red-600">Error: {error}</p> : null}
      <ul className="mt-6 space-y-2">
        {profiles.map((profile) => (
          <li key={profile.id} className="flex items-center gap-3">
            <span className="inline-block h-3 w-3 rounded-full" style={{ background: profile.color }} />
            <span className="font-medium">{profile.name}</span>
            <span className="text-xs text-[color:var(--color-muted)]">{profile.slug}</span>
          </li>
        ))}
      </ul>
    </main>
  )
}
