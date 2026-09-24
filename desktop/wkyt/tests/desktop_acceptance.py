#!/usr/bin/env python3
"""Linux desktop acceptance against a built app; Python standard library only.

Requires a display and WebKitWebDriver. Uses disposable synthetic data, an
isolated passphrase vault, and loopback OAuth/Calendar servers. Never supplies
real credentials or launches the user's browser. See README.md beside this file.
"""
import argparse
import base64
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def wait_for(check, description, timeout=40):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(0.2)
    raise AssertionError(f"Timed out: {description}")


class CalendarMock(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass  # Do not log OAuth URLs or request payloads.

    def respond(self, value):
        body = json.dumps(value).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        if self.path != "/token":
            self.send_error(404)
            return
        self.rfile.read(int(self.headers.get("Content-Length", "0")))
        self.respond({"access_token": "synthetic-access", "refresh_token": "synthetic-refresh",
                      "expires_in": 3600, "token_type": "Bearer"})

    def do_GET(self):
        if self.path.startswith("/calendar/"):
            self.respond({"items": [{"id": "alpha-meeting", "status": "confirmed",
                          "summary": "Project Alpha meeting", "description": "Synthetic acceptance event",
                          "start": {"dateTime": "2026-09-24T14:00:00Z"},
                          "end": {"dateTime": "2026-09-24T15:00:00Z"}}],
                          "nextSyncToken": "synthetic-sync"})
        else:
            self.send_error(404)


class Desktop:
    def __init__(self, port, binary):
        self.base = f"http://127.0.0.1:{port}"
        self.binary = str(binary)
        self.session = None

    def request(self, method, path, data=None):
        encoded = None if data is None else json.dumps(data).encode()
        req = urllib.request.Request(self.base + path, data=encoded, method=method,
                                     headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=45) as response:
                return json.load(response)["value"]
        except urllib.error.HTTPError as error:
            # Driver errors only; never dump page source or credential fields.
            detail = json.load(error).get("value", {})
            raise RuntimeError(detail.get("message", "WebDriver request failed")) from None

    def start(self):
        result = self.request("POST", "/session", {"capabilities": {"alwaysMatch": {
            "webkitgtk:browserOptions": {"binary": self.binary}}}})
        self.session = result["sessionId"]

    def stop(self):
        if self.session:
            self.request("DELETE", f"/session/{self.session}")
            self.session = None

    def js(self, script, *args):
        return self.request("POST", f"/session/{self.session}/execute/sync",
                            {"script": script, "args": list(args)})

    def has(self, text):
        return self.js("return document.body.innerText.includes(arguments[0])", text)

    def click(self, text, scope="body"):
        wait_for(lambda: self.js("""
            const root = document.querySelector(arguments[1]);
            const el = root && [...root.querySelectorAll('button')]
              .find(e => e.textContent.trim() === arguments[0] && !e.disabled);
            if (!el) return false;
            el.click(); return true;
        """, text, scope), f"enabled button {text}")

    def fill(self, selector, value):
        wait_for(lambda: self.js("""
            const el = document.querySelector(arguments[0]);
            if (!el) return false;
            el.value = arguments[1]; el.dispatchEvent(new Event('input', {bubbles:true}));
            return true;
        """, selector, value), f"input {selector}")

    def results(self):
        return self.js("return document.querySelector('.results')?.innerText || ''")

    def save(self, name):
        self.fill('.stream-panel input[placeholder="Alpha this week"]', name)
        self.click("Save as new view", ".stream-panel")
        wait_for(lambda: self.js("return document.querySelector('.saved').innerText.includes(arguments[0])",
                                name), f"saved view {name}")


def walkthrough(app, data):
    passphrase = secrets.token_urlsafe(24)
    app.start()
    wait_for(lambda: app.has("Create Passphrase"), "isolated passphrase setup")
    app.fill('input[type="password"]', passphrase)
    app.click("Create and continue")
    wait_for(lambda: app.js("return !!document.querySelector('.recovery-key')"), "recovery ceremony")
    # Keep the synthetic vault's recovery key only in process memory.
    recovery = app.js("return document.querySelector('.recovery-key').textContent")
    app.click("I saved it — verify me")
    app.fill('input[placeholder="XXXX-XXXX-…"]', recovery)
    app.click("Verify and open vault")
    recovery = None
    wait_for(lambda: app.has("History and saved views"), "unlocked dashboard")
    folder = data / "com.wkyt.app" / "import"
    folder.mkdir(parents=True, exist_ok=True)
    source = folder / "alpha.json"
    source.write_text(json.dumps({"project": "Project Alpha", "note": "original evidence"}))
    app.click("Connect Google Account")
    wait_for(lambda: "Project Alpha meeting" in app.results(), "Calendar ingestion")
    wait_for(lambda: "alpha.json" in app.results(), "file ingestion")
    app.fill('.stream-panel input[placeholder="Project Alpha"]', "Project Alpha")
    app.click("Apply filters", ".stream-panel")
    wait_for(lambda: "Project Alpha meeting" in app.results() and "alpha.json" in app.results(),
             "cross-source query")
    assert app.js("return document.querySelector('.results').innerText.includes('file-import')")
    assert app.js("return document.querySelector('.results').innerText.includes('google-calendar')")
    app.save("Alpha live")
    app.click("Pin this moment", ".stream-panel")
    wait_for(lambda: app.has("Pinned historical view"), "pinned boundary")
    app.save("Alpha pinned")
    original = app.js("return [...document.querySelectorAll('.results pre')].map(e=>e.textContent).join('\\n')")
    assert "original evidence" in original
    source.write_text(json.dumps({"project": "Project Alpha", "note": "corrected evidence, longer"}))
    app.click("Alpha live", ".saved")
    wait_for(lambda: app.js("return [...document.querySelectorAll('.results pre')].some(e=>e.textContent.includes('corrected evidence'))"),
             "live refresh after file correction")
    app.click("Alpha pinned", ".saved")
    wait_for(lambda: app.has("Pinned historical view"), "reopen pinned view")
    wait_for(lambda: app.js("return [...document.querySelectorAll('.results pre')].some(e=>e.textContent.includes('original evidence'))"),
             "original source payload in pinned view")
    assert not app.js("return [...document.querySelectorAll('.results pre')].some(e=>e.textContent.includes('corrected evidence'))")
    print("PASS: file + mock Calendar, overlapping live/pinned views, source correction", flush=True)

    app.stop()
    app.start()
    wait_for(lambda: app.has("Unlock Vault"), "encrypted vault reopen")
    app.fill('input[type="password"]', passphrase)
    app.click("Unlock")
    wait_for(lambda: app.has("Alpha pinned") and app.has("Alpha live"), "persisted view definitions")
    app.click("Alpha pinned", ".saved")
    wait_for(lambda: app.js("return [...document.querySelectorAll('.results pre')].some(e=>e.textContent.includes('original evidence'))"),
             "pinned history after restart")
    app.js("""
        const row = [...document.querySelectorAll('.saved li')].find(e=>e.textContent.includes('Alpha pinned'));
        [...row.querySelectorAll('button')].find(e=>e.textContent.trim()==='Remove view').click();
    """)
    wait_for(lambda: not app.js("return document.querySelector('.saved').innerText.includes('Alpha pinned')"),
             "definition removal")
    app.click("Alpha live", ".saved")
    wait_for(lambda: "alpha.json" in app.results() and "Project Alpha meeting" in app.results(),
             "evidence retained after removing view")
    assert source.exists()
    print("PASS: reopen, retained historical payload, remove view without deleting either source", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--screenshot", type=Path, help="Optional screenshot of synthetic results only")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    driver_binary = shutil.which("WebKitWebDriver")
    if not driver_binary:
        parser.error("WebKitWebDriver must be installed")
    with tempfile.TemporaryDirectory(prefix="wkyt-acceptance-") as temp:
        root = Path(temp)
        shim = root / "bin"
        shim.mkdir()
        # open::that uses xdg-open on Linux. Only accept the local synthetic OAuth
        # URL and send its callback directly; never launch a browser/account login.
        opener = shim / "xdg-open"
        opener.write_text("""#!/usr/bin/env python3
import os,sys,time,urllib.parse,urllib.request
url=urllib.parse.urlparse(sys.argv[1])
assert url.hostname == '127.0.0.1' and url.path == '/authorize'
q=urllib.parse.parse_qs(url.query)
redirect=urllib.parse.urlparse(q['redirect_uri'][0])
assert redirect.hostname in ('127.0.0.1','localhost')
if os.fork():
    sys.exit(0)
callback=q['redirect_uri'][0]+'?'+urllib.parse.urlencode({'code':'synthetic-code','state':q['state'][0]})
for _ in range(100):
    try:
        urllib.request.urlopen(callback,timeout=1).close()
        break
    except OSError:
        time.sleep(.1)
else:
    sys.exit(1)
""")
        opener.chmod(0o700)
        server = ThreadingHTTPServer(("127.0.0.1", 0), CalendarMock)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        mock = f"http://127.0.0.1:{server.server_port}"
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        env = dict(os.environ)
        env.update({"XDG_DATA_HOME": str(root / "data"), "XDG_CONFIG_HOME": str(root / "config"),
                    "XDG_CACHE_HOME": str(root / "cache"),
                    "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(root / "no-session-bus"),
                    "TAURI_WEBVIEW_AUTOMATION": "true", "PATH": str(shim) + os.pathsep + env["PATH"],
                    "WKYT_GOOGLE_CLIENT_ID": "synthetic-client", "WKYT_GOOGLE_CLIENT_SECRET": "synthetic-secret",
                    "WKYT_MOCK_GOOGLE_AUTH_URL": mock + "/authorize", "WKYT_MOCK_GOOGLE_TOKEN_URL": mock + "/token",
                    "WKYT_MOCK_CALENDAR_API_BASE": mock + "/calendar"})
        # Suppress all app/driver output: assertions report stages, never secrets.
        driver = subprocess.Popen([driver_binary, "--host=127.0.0.1", f"--port={port}"],
                                  env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        app = Desktop(port, binary)
        try:
            def ready():
                try:
                    return app.request("GET", "/status").get("ready")
                except urllib.error.URLError:
                    return False
            wait_for(ready, "WebKitWebDriver startup")
            walkthrough(app, root / "data")
            if args.screenshot:
                app.js("document.querySelector('.stream-panel').scrollIntoView()")
                encoded = app.request("GET", f"/session/{app.session}/screenshot")
                args.screenshot.write_bytes(base64.b64decode(encoded))
        finally:
            try:
                app.stop()
            finally:
                driver.terminate()
                driver.wait(timeout=10)
                server.shutdown()


if __name__ == "__main__":
    main()
