/**
 * Dollars and cents, without the cents from $100 up or on whole dollars, and
 * `<$0.01` for an amount too small to show in cents.
 */
export function formatUsd(usd: number): string {
  if (usd > 0 && usd < 0.005) {
    return '<$0.01'
  }
  const digits = usd >= 100 || Number.isInteger(usd) ? 0 : 2
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    maximumFractionDigits: digits,
    minimumFractionDigits: digits,
  }).format(usd)
}

/**
 * A share as a whole percent, with a share too small to round up as `<1%`.
 */
export function formatShare(share: number): string {
  const percent = Math.round(share * 100)
  return percent === 0 && share > 0 ? '<1%' : `${percent}%`
}

/**
 * An amount in a currency's minor units, as a localised currency string. The
 * amount is only divided down at the last moment, so the integer the backend
 * sent stays exact. A currency `Intl` doesn't know shows as the bare number
 * with its code beside it.
 */
export function formatMoney(amountMinor: number, currency: string, exponent: number): string {
  const amount = amountMinor / 10 ** exponent
  try {
    return new Intl.NumberFormat(undefined, {
      style: 'currency',
      currency,
      minimumFractionDigits: exponent,
      maximumFractionDigits: exponent,
    }).format(amount)
  } catch {
    return `${amount.toFixed(exponent)} ${currency}`
  }
}
