import { useEffect, useRef, useState } from 'react'

import { ConsentManagerProvider, useConsentManager } from '@c15t/react'

import { grantConsent, initAnalytics, revokeConsent, track } from '../lib/analytics'

function Banner() {
  const consent = useConsentManager()
  const [leaving, setLeaving] = useState(false)
  const [forcedOpen, setForcedOpen] = useState(false)
  const dismissTimer = useRef<number | null>(null)

  useEffect(() => {
    function handleOpen() {
      setLeaving(false)
      setForcedOpen(true)
      consent.setActiveUI('banner', { force: true })
    }
    window.addEventListener('cookie-banner:open', handleOpen)
    return () => window.removeEventListener('cookie-banner:open', handleOpen)
  }, [consent])

  useEffect(() => {
    return () => {
      if (dismissTimer.current !== null) {
        window.clearTimeout(dismissTimer.current)
        dismissTimer.current = null
      }
    }
  }, [])

  const show = forcedOpen || consent.activeUI === 'banner'

  if (!show) {
    return null
  }

  function dismiss(callback: () => void) {
    if (dismissTimer.current !== null) {
      window.clearTimeout(dismissTimer.current)
    }
    setLeaving(true)
    dismissTimer.current = window.setTimeout(() => {
      callback()
      setLeaving(false)
      setForcedOpen(false)
      dismissTimer.current = null
    }, 160)
  }

  function accept() {
    dismiss(() => {
      void consent.saveConsents('all')
    })
  }

  function decline() {
    dismiss(() => {
      void consent.saveConsents('necessary')
    })
  }

  return (
    <div role="dialog" aria-label="Cookie preferences" data-leaving={leaving || undefined} className="cookie-banner">
      <p>Anonymous page analytics, off by default.</p>
      <div className="cookie-banner-actions">
        <button type="button" onClick={accept} className="btn-accept">
          Accept
        </button>
        <button type="button" onClick={decline} className="btn-decline">
          Decline
        </button>
      </div>
    </div>
  )
}

function AnalyticsBridge() {
  const consent = useConsentManager()

  useEffect(() => {
    void initAnalytics().then(() => {
      if (consent.has('measurement')) {
        grantConsent()
      } else {
        revokeConsent()
      }
    })
  }, [consent])

  useEffect(() => {
    function onDownload() {
      track('download_click')
    }
    function onGithub() {
      track('github_click')
    }
    function onInstallCopy(event: Event) {
      const detail = (event as CustomEvent<{ target: string }>).detail
      track('install_copy', { target: detail?.target })
    }
    function onFaqOpen(event: Event) {
      const detail = (event as CustomEvent<{ index: number }>).detail
      track('faq_open', { question_index: detail?.index })
    }
    const cta = document.getElementById('download-cta')
    const githubLinks = document.querySelectorAll('a[href="https://github.com/bartekczyz/ai-profiles"]')
    cta?.addEventListener('click', onDownload)
    githubLinks.forEach((link) => {
      link.addEventListener('click', onGithub)
    })
    window.addEventListener('install:copy', onInstallCopy)
    window.addEventListener('faq:open', onFaqOpen)
    return () => {
      cta?.removeEventListener('click', onDownload)
      githubLinks.forEach((link) => {
        link.removeEventListener('click', onGithub)
      })
      window.removeEventListener('install:copy', onInstallCopy)
      window.removeEventListener('faq:open', onFaqOpen)
    }
  }, [])

  return null
}

export default function ConsentRoot() {
  return (
    <ConsentManagerProvider
      options={{
        mode: 'offline',
        consentCategories: ['necessary', 'measurement'],
      }}
    >
      <Banner />
      <AnalyticsBridge />
    </ConsentManagerProvider>
  )
}
