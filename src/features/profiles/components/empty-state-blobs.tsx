/**
 * Two overlapping translucent blobs — orange + steel — rendered via SVG
 * with mix-blend-mode multiply so the overlap reads as a warm bruise.
 * Purely decorative, sits above the empty-state headline.
 */
export function EmptyStateBlobs() {
  return (
    <svg width="124" height="90" viewBox="0 0 124 90" fill="none" aria-hidden className="mb-7">
      <title>Two overlapping blobs</title>
      <g style={{ mixBlendMode: 'multiply' }}>
        <circle cx="39" cy="45" r="39" fill="var(--color-profile-orange)" fillOpacity="0.55" />
        <circle cx="85" cy="45" r="39" fill="var(--color-profile-steel)" fillOpacity="0.55" />
      </g>
    </svg>
  )
}
