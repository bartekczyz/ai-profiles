/**
 * Whether the offer to repair a profile's sessions shows: while some session
 * needs repair that the user hasn't said not now to. Dismissing it covers the
 * sessions that needed repair then, so a new one brings it back.
 */
export function repairOfferShown(needingRepair: ReadonlyArray<string>, dismissed: ReadonlyArray<string>): boolean {
  return needingRepair.some((id) => !dismissed.includes(id))
}
