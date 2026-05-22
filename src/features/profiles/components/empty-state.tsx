import { Button } from '@/design/ui/button'

type Props = {
  onCreate: () => void
}

export function EmptyState({ onCreate }: Props) {
  return (
    <div className="flex h-full flex-1 flex-col items-center justify-center bg-background px-12 text-center">
      <svg width="120" height="80" viewBox="0 0 120 80" fill="none" aria-hidden="true">
        <title>two-overlapping-circles</title>
        <circle cx="48" cy="40" r="32" fill="var(--color-profile-orange)" fillOpacity="0.35" />
        <circle cx="76" cy="40" r="32" fill="var(--color-profile-steel)" fillOpacity="0.35" />
      </svg>
      <h2 className="mt-8 text-xl font-semibold">No profiles yet</h2>
      <p className="mt-2 max-w-sm text-sm text-muted-foreground">
        Each profile is one Claude account, with its own desktop app launcher and CLI wrapper.
      </p>
      <Button className="mt-6" onClick={onCreate}>
        Create your first profile
      </Button>
    </div>
  )
}
