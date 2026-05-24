/**
 * No-flash theme script. Runs synchronously in <head> before the body
 * paints. Reads localStorage first (for a future explicit toggle),
 * falls back to prefers-color-scheme.
 */
export const themeScript = `
(function() {
  try {
    var stored = localStorage.getItem('theme');
    var prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    var theme = stored === 'light' || stored === 'dark' ? stored : (prefersDark ? 'dark' : 'light');
    document.documentElement.setAttribute('data-theme', theme);
  } catch (error) {
    document.documentElement.setAttribute('data-theme', 'light');
  }
})();
`.trim()
