import '@testing-library/jest-dom/vitest'

// The app's primary platform is macOS. Pin happy-dom's userAgent so
// shortcut chips render the Mac symbol form (⌘, ⌥, ⌫) in component
// tests. Individual tests can override via Object.defineProperty.
Object.defineProperty(globalThis.navigator, 'userAgent', {
  value: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15',
  configurable: true,
})
