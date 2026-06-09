import type { ReactNode } from 'react'

import { ProfileDetailHintStrip } from './profile-detail-hint-strip'

type Props = {
  children: ReactNode
}

/**
 * Outer chrome shared by every detail page variant: a non-scrolling
 * <main> that contains a scrollable content area and a pinned hint strip
 * at the bottom. Variants supply the section content as children — the
 * shell does not assume anything about what they render.
 */
export function ProfileDetailShell({ children }: Props) {
  return (
    <main className="flex flex-1 flex-col overflow-hidden">
      <div className="flex-1 overflow-y-auto px-10 pt-10">
        <div className="mx-auto w-full max-w-[640px]">{children}</div>
      </div>
      <ProfileDetailHintStrip />
    </main>
  )
}
