import type { ProfileAccount } from '@/lib/types'

/**
 * How a profile's account reads in the detail header's sub-line: the email,
 * else the person's name. The plan rides along when there is one, and the
 * organization is left for the tooltip — it repeats the email in personal
 * accounts ("ada@example.com's Organization").
 */
export function accountLabel(account: ProfileAccount | null | undefined): string | null {
  if (account === undefined) {
    return null
  }
  if (account === null) {
    return 'Not signed in'
  }
  const who = account.email ?? account.name
  if (who === null) {
    return account.plan
  }
  return account.plan ? `${who} · ${account.plan}` : who
}

/** The fuller account, for the sub-line's tooltip. */
export function accountTitle(account: ProfileAccount | null | undefined): string | undefined {
  if (!account) {
    return
  }
  const parts = [account.name, account.email, account.organization, account.plan].filter(
    (part): part is string => part !== null,
  )
  return parts.length > 0 ? parts.join(' · ') : undefined
}
