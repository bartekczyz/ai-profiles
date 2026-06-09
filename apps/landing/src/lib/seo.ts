import { type FaqEntry, faqEntries } from '../data/faq'

export interface SoftwareApplicationOptions {
  siteUrl: string
  version: string
}

export function softwareApplicationLd({ siteUrl, version }: SoftwareApplicationOptions): Record<string, unknown> {
  return {
    '@context': 'https://schema.org',
    '@type': 'SoftwareApplication',
    name: 'ai-profiles',
    description: 'Free, open-source profile manager for the Claude desktop app and the Claude Code CLI on macOS.',
    applicationCategory: 'DeveloperApplication',
    operatingSystem: 'macOS 12+',
    offers: {
      '@type': 'Offer',
      price: '0',
      priceCurrency: 'USD',
    },
    softwareVersion: version,
    license: 'https://opensource.org/licenses/MIT',
    url: siteUrl,
    downloadUrl: 'https://github.com/bartekczyz/ai-profiles/releases/latest',
    author: {
      '@type': 'Person',
      name: 'Bartek Czyż',
    },
  }
}

export function faqPageLd(entries: ReadonlyArray<FaqEntry> = faqEntries): Record<string, unknown> {
  return {
    '@context': 'https://schema.org',
    '@type': 'FAQPage',
    mainEntity: entries.map((entry) => ({
      '@type': 'Question',
      name: entry.question,
      acceptedAnswer: {
        '@type': 'Answer',
        text: entry.answer,
      },
    })),
  }
}
