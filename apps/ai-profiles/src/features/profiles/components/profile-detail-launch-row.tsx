import type { ReactNode } from 'react'
import type { AppId } from '@/lib/app-registry'

import { ProfileDetailApiCost } from './profile-detail-api-cost'

type Props = {
  app: AppId
  profileId: string
  /**
   * The launch panel, already wrapped in its Suspense boundary.
   */
  surfaces: ReactNode
}

/**
 * The launch panel and the cost card, sharing a row under the usage card;
 * with no cost to show, the panel takes the whole row. When the pane is too
 * narrow for both side by side, they stack at full width, so the panel's
 * buttons never clip.
 */
export function ProfileDetailLaunchRow({ app, profileId, surfaces }: Props) {
  return (
    <div className="@container mb-3">
      <div className="flex flex-col gap-3 @[620px]:flex-row @[620px]:items-stretch">
        <div className="min-w-0 @[620px]:flex-[2] @[620px]:basis-0">{surfaces}</div>
        <ProfileDetailApiCost app={app} profileId={profileId} />
      </div>
    </div>
  )
}
