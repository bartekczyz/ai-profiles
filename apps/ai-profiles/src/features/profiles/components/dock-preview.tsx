type Props = {
  /**
   * The launcher's icon as a data URL, or `null` while it is rendered.
   */
  icon: string | null
}

/**
 * Two stand-ins for other apps' icons, one either side of the profile's, so it
 * is seen among others, as in the Dock. Plain on purpose: they must not read as
 * any real app.
 */
const leftApp = 'bg-[linear-gradient(160deg,#9fb4cc,#6b8db5)]'
const rightApp = 'bg-[linear-gradient(160deg,#e3d9c3,#bfae8c)] dark:bg-[linear-gradient(160deg,#6e6656,#4a4438)]'

/**
 * A stand-in app's icon, filling 80% of its tile, as an app icon's artwork does.
 */
function OtherApp({ tile }: { tile: string }) {
  return (
    <span aria-hidden className="grid h-9 w-9 place-items-center">
      <span className={`h-[80%] w-[80%] rounded-[7px] shadow-[inset_0_1px_0_rgba(255,255,255,0.35)] ${tile}`} />
    </span>
  )
}

/**
 * A small picture of the launcher's icon in the Dock: on the window's own
 * background (the cream wash with the app's accent and steel), a frosted Dock
 * holding two stand-in apps and the profile's, running.
 */
export function DockPreview({ icon }: Props) {
  return (
    <div className="relative flex h-[84px] w-full items-end justify-center overflow-hidden rounded-lg border border-border-soft bg-(--color-base) bg-[radial-gradient(70%_120%_at_18%_12%,color-mix(in_oklab,var(--app-accent)_45%,transparent),transparent_80%),radial-gradient(65%_120%_at_82%_88%,color-mix(in_oklab,var(--color-profile-steel)_35%,transparent),transparent_80%)] pb-2">
      <div className="flex items-end gap-1.5 rounded-[12px] border border-white/60 bg-white/45 px-1.5 pt-1.5 pb-2 shadow-[0_4px_14px_-6px_rgba(0,0,0,0.25)] backdrop-blur-md dark:border-white/15 dark:bg-white/10">
        <OtherApp tile={leftApp} />
        {/* Relative, so the running dot hangs below the icon without taking up
            room: it would otherwise lift this icon above the others. */}
        <span className="relative">
          {icon === null ? (
            <span aria-hidden className="grid h-9 w-9 place-items-center">
              <span className="h-[80%] w-[80%] animate-pulse rounded-[7px] bg-black/10 dark:bg-white/15" />
            </span>
          ) : (
            <img src={icon} alt="Launcher icon preview" className="block h-9 w-9 object-contain" />
          )}
          {/* The Dock's dot under a running app. */}
          <span
            aria-hidden
            className="absolute -bottom-[5px] left-1/2 h-[3px] w-[3px] -translate-x-1/2 rounded-full bg-ink/70"
          />
        </span>
        <OtherApp tile={rightApp} />
      </div>
    </div>
  )
}
