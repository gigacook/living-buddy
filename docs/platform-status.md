# Platform verification status

This page records what has actually been built and run, as opposed to configured. "Emulated" means a desktop browser with a phone-sized viewport and touch emulation, which is **not** proof of behavior on a real device.

| Target | Status | How it was verified |
| --- | --- | --- |
| Rust core + server (Linux x86_64) | Verified | `cargo fmt --check`, `cargo clippy -D warnings`, unit and integration tests |
| Web app in Chromium (desktop 1280×860) | Verified | Playwright end-to-end tests + axe WCAG 2.2 AA scans, light and dark theme |
| Web app at phone size (Pixel 7 profile, Chromium) | Emulated | Same Playwright suite with a mobile viewport and touch; no horizontal page scrolling |
| Native app on Linux (Tauri 2, WebKitGTK 2.52) | Built and launched | Compiled with the Linux SDK, launched under a virtual display; the UI rendered and reached the in-process API (database created). Not packaged as `.deb`/AppImage, not tested interactively |
| Safari / WebKit browsers | Not verified | No macOS or Playwright WebKit build available in the build environment. The Linux WebKitGTK run above shares the WebKit engine but is not Safari |
| macOS app | Configuration only | Valid Tauri config and icons; not built (needs macOS + Xcode) |
| Windows app | Configuration only | Not built (needs Windows + WebView2 + MSVC) |
| iOS app | Configuration only | Not built or run on a simulator or device (needs macOS + Xcode); `tauri ios init` not run |
| Android app | Configuration only | Not built or run (needs Android SDK/NDK); `tauri android init` not run |
| Docker image | See below | |

## Docker

The `Dockerfile` and `deploy/docker-compose.yml` are provided. Whether the image build was verified in the original build environment is recorded in [`build-index.json`](../build-index.json) under `deploy.docker`.

## Known gaps

- Scheduled OS notifications that fire while the app is closed are not implemented on any platform.
- Mobile background behavior (iOS/Android) is untested.
- Real provider accounts (Google, Microsoft, Slack, Anthropic) were not used; adapters are tested against local mock servers.
