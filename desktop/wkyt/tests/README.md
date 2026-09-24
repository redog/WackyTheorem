# Desktop acceptance (Linux)

This test drives the built Tauri app through WebKitWebDriver. It uses Python's standard library and adds no application dependency. Run it on a Linux desktop with WebKitWebDriver installed and a working display:

```sh
# From desktop/wkyt; build the exact checkout being tested.
npm ci
npm run tauri build -- --debug --no-bundle -- --locked
python3 tests/desktop_acceptance.py --binary ../../target/debug/wkyt
```

The test creates a disposable app-data directory, disables access to the normal session keychain, and enters a random passphrase through the real UI. It completes recovery verification without logging or saving the key. File records and Calendar responses are synthetic. Existing connector mock-endpoint overrides route OAuth and Calendar traffic to loopback; an isolated `xdg-open` shim handles only that synthetic callback without launching a browser. No real Google account or personal vault is needed.

The walkthrough covers a mixed file/Calendar query, overlapping saved views, automatic live refresh after source correction, pinned original payloads, encrypted-vault reopen, persisted definitions, and removing a view while retaining both sources. Failures stop the run; no screenshot or page dump is taken during the recovery ceremony. Optional `--screenshot /absolute/path.png` captures synthetic results only after all checks pass.

Use an ordinary build made without compile-time Google credentials, since the existing app prefers those over environment overrides. The Ubuntu CI job runs this test against `target/release/wkyt` after workspace tests and desktop packaging, and before artifact upload. It uses `xvfb-run` with the X11 backend and software rendering, and fails the job on any failed assertion or ten-minute timeout. Fedora, Windows, and macOS retain their existing checks and builds. It does not certify production Google OAuth, OS-keychain integration, other desktop platforms' interactions, or the broader action-history invariant.

Protocol setup follows [Tauri's WebDriver documentation](https://v2.tauri.app/develop/tests/webdriver/). On Linux the native driver can accept the application through `webkitgtk:browserOptions`, the same mapping used by `tauri-driver`.

To reproduce the CI virtual-display setup locally (requires Xvfb and xauth):

```sh
# From the repository root; use a freshly built binary.
GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 xvfb-run -a -s '-screen 0 1280x1024x24' \
  python3 desktop/wkyt/tests/desktop_acceptance.py --binary target/debug/wkyt
```

CI installs `python3`, `webkit2gtk-driver`, `xvfb`, and `xauth` on Ubuntu. It does not request screenshots or upload app/driver logs. Package installation or display failures are infrastructure failures, not acceptance passes.
