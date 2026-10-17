# Clean view → "Web browser caches" → "Advanced: clean one website"
# (BrowserSites.svelte, crates/nfd-browsers). Every key must exist in vi/browsers.ftl.

br-title = Advanced: clean one website
br-hint = Remove the cache of chosen websites only, in each browser profile
br-intro = Tick websites to remove only their own cache: compiled scripts (Code Cache), Service Worker caches and, in Firefox, the web cache. Cookies, logins, Local Storage, IndexedDB, history, passwords and extensions are never read or touched. Close the browser first; everything goes to the quarantine and can be restored from Activity for 7 days.
br-blockfile-note = Chromium's web (HTTP) cache on Windows uses the "blockfile" format: all sites share one index and data files, so removing one site there would mean editing them in place and could corrupt the whole cache. Its part is shown for information and stays; it is cleaned only as a whole, with the rule above.
br-loading = Reading the browser caches… (read-only)
br-none = No browser cache with websites found.
br-search = Search websites
br-rescan = Read the caches again
br-profile = profile { $name }
br-sites = { $count } website(s)
br-running = { $exe } is running — close it first; nothing is cleaned in this browser while it runs.
br-site-listed = + { $size } in the web cache (stays)
br-listed-only-hint = Only in the blockfile web cache: listed, not cleaned per site.
br-no-match = No website matches the search.
br-store-http-cache = Web cache
br-store-code-cache-js = JavaScript cache
br-store-code-cache-wasm = WebAssembly cache
br-store-cache-storage = Service Worker cache
br-store-firefox-cache = Firefox cache
br-more = Show { $count } more
br-clean-n = Clean { $count } website(s) ({ $size })
br-clean-hint = If you click: you see exactly what goes first; the entries move to the quarantine.
br-confirm-title = Clean these websites' cache?
br-confirm-body = { $entries } cache entries ({ $size }) of { $count } website(s) will move to the quarantine:
br-confirm-store = { $entries } · { $size }
br-confirm-index = { $count } cache index file(s) go too: the browser rebuilds them on its next start.
br-confirm-listed = { $size } of these websites stays in the blockfile web cache (not edited per site).
br-confirm-running = Close { $apps } first: nothing is cleaned in it while it runs.
br-confirm-nothing = Nothing to clean for these websites right now.
br-confirm-note = Only whole cache entries move; cookies, logins and site data stay. Everything can be restored from Activity for 7 days.
br-done = Website cache cleaned: { $size } · { $done } item(s).
activity-kind-browser-site = Website cache cleaned
reason-app-running = The app was started while cleaning; skipped.
