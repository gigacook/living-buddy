# Local development

## Prerequisites

- Rust 1.80 or newer (`rustup`), with `rustfmt` and `clippy`
- Node.js 20 or newer and npm
- For the native app only: the Tauri 2 prerequisites for your OS (see "Native app" below)

## First run

```bash
npm install
cargo run -p tendly-server -- seed-demo     # optional synthetic demo data
cargo run -p tendly-server -- serve          # API on http://127.0.0.1:7878
npm run dev                                  # UI on http://127.0.0.1:5173 (proxies /api)
```

Data lives in `./data` by default (`TENDLY_DATA_DIR`). The encryption key for stored tokens is generated at `./data/secrets/encryption.key` the first time; see [security.md](security.md) for better places to keep it.

## Useful commands

| Task | Command |
| --- | --- |
| Rust format / lint | `cargo fmt --all` / `cargo clippy --workspace --all-targets -- -D warnings` |
| Rust tests | `cargo test --workspace` |
| Regenerate TypeScript contracts | `npm run contracts` (runs ts-rs export tests, then rebuilds `packages/contracts/src/index.ts`) |
| Web lint / types / unit tests | `npm run lint`, `npm run typecheck`, `npm test` |
| Production web build | `npm run build` |
| End-to-end + accessibility | `cd apps/web && npx playwright test` (starts a throwaway server on port 7899) |
| Serve the production build | `TENDLY_WEB_DIR=apps/web/dist cargo run -p tendly-server -- serve` |

## Native app (Tauri 2)

`apps/native/src-tauri` is a separate Cargo project so the main workspace never needs GUI SDKs.

```bash
cd apps/native
npx tauri dev       # uses the Vite dev server
npx tauri build     # bundles apps/web/dist into the app
```

Linux needs `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `librsvg2-dev`, `libsoup-3.0-dev`. macOS needs Xcode command-line tools; Windows needs WebView2 and MSVC build tools; iOS needs Xcode; Android needs the Android SDK/NDK (`npx tauri android init` / `npx tauri ios init` generate the mobile projects). See [platform-status.md](platform-status.md) for what has actually been built and run.

The native app does not open a network port. The UI calls a single Tauri command (`api_request`) that is dispatched in-process to the same Rust router used by the server.
