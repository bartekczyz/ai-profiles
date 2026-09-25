import { useDependencies } from '@/features/dependencies/api/use-dependencies'
import { shouldShowPathBanner } from '@/features/onboarding/lib/path-banner'
import { useProfiles } from '@/features/profiles/api/use-profiles'
import { useAppState } from '@/lib/app-state/use-app-state'

import { PathSetupBanner } from './path-setup-banner'

/**
 * Shows the PATH setup banner when a CLI wrapper needs `~/.local/bin` on
 * PATH and the user hasn't dismissed it recently.
 */
export function PathSetupBannerHost() {
  const appState = useAppState()
  const dependencies = useDependencies()
  const profiles = useProfiles()

  const show = shouldShowPathBanner({
    welcomeShown: appState.state.welcomeShown,
    localBinOnPath: dependencies.deps.localBinOnPath,
    anyCliProfile: profiles.profiles.some((profile) => profile.surfaces.cli),
    dismissedAt: appState.state.pathBannerDismissedAt,
    now: Date.now(),
  })
  if (!show) {
    return null
  }

  return (
    <PathSetupBanner
      onFixed={async () => {
        await dependencies.refresh()
      }}
      onDismiss={async () => {
        await appState.update({ pathBannerDismissedAt: new Date().toISOString() })
      }}
    />
  )
}
