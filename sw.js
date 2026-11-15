// Offline use (SPEC-Rust §14.2).
//
// Trunk names the app's JavaScript and WebAssembly after a hash of their
// contents, so a file with a hashed name never changes: those are served from
// the cache once fetched. The page itself is fetched from the network first,
// so a reload always gets the newest version when online, and from the cache
// when offline. Each time a fresh page arrives, hashed files it no longer uses
// are deleted from the cache.
//
// Registered by index.html with a relative URL, so its scope is the app's
// base path (/<repo>/ on GitHub Pages).

const CACHE = "pathwaydesigner";
// `pathway-designer-<hash>.js` or `pathway-designer-<hash>_bg.wasm`. The hash
// has up to 16 hex digits: Trunk drops leading zeros.
const HASHED = /-[0-9a-f]{1,16}(_bg)?\.(js|wasm)$/;

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

self.addEventListener("fetch", (event) => {
  const request = event.request;
  const scope = self.registration.scope;
  if (request.method !== "GET" || !request.url.startsWith(scope)) {
    return;
  }
  const path = new URL(request.url).pathname;
  if (HASHED.test(path)) {
    event.respondWith(cacheFirst(request));
  } else {
    event.respondWith(networkFirst(request, scope));
  }
});

async function cacheFirst(request) {
  const cache = await caches.open(CACHE);
  const hit = await cache.match(request);
  if (hit) {
    return hit;
  }
  const response = await fetch(request);
  if (response.ok) {
    await cache.put(request, response.clone());
  }
  return response;
}

async function networkFirst(request, scope) {
  const cache = await caches.open(CACHE);
  // The page is kept under one key, however it was asked for.
  const isPage = request.mode === "navigate" || request.url === scope
    || request.url === scope + "index.html";
  const key = isPage ? scope : request;
  try {
    const response = await fetch(request, { cache: "no-cache" });
    if (response.ok) {
      await cache.put(key, response.clone());
      if (isPage) {
        await prune(cache, await response.clone().text());
      }
    }
    return response;
  } catch (error) {
    const hit = await cache.match(key);
    if (hit) {
      return hit;
    }
    throw error;
  }
}

// Deletes cached hashed files the current page doesn't use.
async function prune(cache, page) {
  for (const request of await cache.keys()) {
    const path = new URL(request.url).pathname;
    const name = path.slice(path.lastIndexOf("/") + 1);
    if (HASHED.test(path) && !page.includes(name)) {
      await cache.delete(request);
    }
  }
}
