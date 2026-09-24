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

Use an ordinary build made without compile-time Google credentials, since the existing app prefers those over environment overrides. The test is currently an explicit Linux acceptance check, not part of the four-platform CI matrix. It does not certify production Google OAuth, OS-keychain integration, other desktop platforms' interactions, or the broader action-history invariant.

Protocol setup follows [Tauri's WebDriver documentation](https://v2.tauri.app/develop/tests/webdriver/). On Linux the native driver can accept the application through `webkitgtk:browserOptions`, the same mapping used by `tauri-driver`.
