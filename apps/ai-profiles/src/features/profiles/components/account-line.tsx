import type { AccountStatus } from '@/lib/types'

/**
 * How a profile's account reads in the detail header's sub-line: the email,
 * else the person's name. The plan rides along when there is one, and the
 * organization is left for the tooltip — it repeats the email in personal
 * accounts ("ada@example.com's Organization"). Nothing while it's read, or
 * when who is signed in can't be told: saying "Not signed in" then could be
 * wrong.
 */
export function accountLabel(status: AccountStatus | undefined): string | null {
  if (status === undefined || status.status === 'unknown') {
    return null
  }
  if (status.status === 'signedOut') {
    return 'Not signed in'
  }
  const { account } = status
  const who = account.email ?? account.name
  if (who === null) {
    return account.plan
  }
  return account.plan ? `${who} · ${account.plan}` : who
}

/** The fuller account, for the sub-line's tooltip. */
export function accountTitle(status: AccountStatus | undefined): string | undefined {
  if (status?.status !== 'signedIn') {
    return
  }
  const { account } = status
  const parts = [account.name, account.email, account.organization, account.plan].filter(
    (part): part is string => part !== null,
  )
  return parts.length > 0 ? parts.join(' · ') : undefined
}
