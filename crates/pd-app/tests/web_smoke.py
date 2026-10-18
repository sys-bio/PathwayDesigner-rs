"""A browser check of the web build (SPEC-Rust §12, §14.2): the production
build, served from a sub-path as GitHub Pages serves it, in headless Chromium.

    python crates/pd-app/tests/web_smoke.py <dist folder> [screenshot.png]

Checks that the app starts without errors, the service worker takes over,
the app starts again with the network off, the "new version" banner appears
when the published page names another build, and the licences page is
there. Needs `pip install playwright` and `playwright install chromium`.
"""
import functools
import http.server
import os
import re
import shutil
import sys
import tempfile
import threading

from playwright.sync_api import sync_playwright

BASE = "/PathwayDesigner/"
dist = sys.argv[1]
shot = sys.argv[2] if len(sys.argv) > 2 else None

# Serve the build at /PathwayDesigner/, as Pages does.
root = tempfile.mkdtemp()
site = os.path.join(root, BASE.strip("/"))
shutil.copytree(dist, site)


class Quiet(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map,
                      ".wasm": "application/wasm", ".js": "text/javascript"}

    def log_message(self, *args):
        pass


server = http.server.ThreadingHTTPServer(
    ("127.0.0.1", 0), functools.partial(Quiet, directory=root))
threading.Thread(target=server.serve_forever, daemon=True).start()
url = f"http://127.0.0.1:{server.server_port}{BASE}"

failures = []


def check(ok, what):
    print(("ok   " if ok else "FAIL ") + what, flush=True)
    if not ok:
        failures.append(what)


with sync_playwright() as p:
    browser = p.chromium.launch(
        args=["--enable-unsafe-swiftshader", "--use-angle=swiftshader"])
    context = browser.new_context(viewport={"width": 1400, "height": 900})
    page = context.new_page()
    errors = []
    page.on("pageerror", lambda e: errors.append(str(e)))
    page.on("console", lambda m: m.type == "error" and errors.append(m.text))

    def started():
        page.wait_for_function("!document.getElementById('loading_text')", timeout=90000)

    page.goto(url)
    started()
    check(not errors, f"the app starts without errors {errors}")
    check(page.evaluate("document.title").endswith("PathwayDesigner"), "the window title is set")

    # The service worker takes over once ready (or never, if it's missing).
    ready = page.evaluate("""Promise.race([
        navigator.serviceWorker.ready.then(() => true),
        new Promise(done => setTimeout(() => done(false), 15000)),
    ])""")
    check(ready, "the service worker is installed")
    page.reload()
    started()
    check(page.evaluate("!!navigator.serviceWorker.controller"), "the service worker controls the page")
    if shot:
        page.screenshot(path=shot)

    # Offline: the app comes from the cache.
    context.set_offline(True)
    errors.clear()
    try:
        page.reload()
        started()
        check(not errors, f"the app starts offline {errors}")
    except Exception as e:
        check(False, f"the app starts offline ({str(e).splitlines()[0]})")
    context.set_offline(False)
    page.goto(url)
    started()

    # The licences page is published next to the app.
    status = page.evaluate("fetch('THIRD-PARTY.html', {cache: 'no-store'}).then(r => r.status)")
    check(status == 200, "THIRD-PARTY.html is there")

    # A new build is published: the banner asks for a reload.
    check(not page.is_visible("#update_banner"), "no banner while the build is current")
    index = os.path.join(site, "index.html")
    with open(index, encoding="utf-8") as f:
        text = f.read()
    # Up to 16 hex digits: Trunk drops leading zeros.
    running = re.search(r"pathway-designer-([0-9a-f]{1,16})\.js", text).group(1)
    with open(index, "w", encoding="utf-8") as f:
        f.write(text.replace(running, "0123456789abcdef"))
    page.evaluate("checkForUpdate()")
    page.wait_for_selector("#update_banner", state="visible", timeout=10000)
    check(page.is_visible("#update_banner"), "the banner appears for a new build")
    browser.close()

server.shutdown()
shutil.rmtree(root, ignore_errors=True)
print(f"{'all checks pass' if not failures else str(len(failures)) + ' failed'}")
sys.exit(1 if failures else 0)
