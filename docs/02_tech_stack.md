# Tech Stack

_Last updated: 2026-11-21_

## Platform implementation status

| Platform | Shell and core | Status |
|---|---|---|
| macOS | Tauri 2 + Rust core + React UI | Existing implementation and release path. |
| Windows | Python 3.12 + pywebview + pystray + React UI | New implementation in `hitch/`; packaged with PyInstaller one-folder. |

Windows uses the React UI in `src/` and a Python bridge/backend in `hitch/`.
Rust/Tauri remains the macOS shell and the source reference for shared behavior; the Windows build does not invoke Cargo or Tauri.

## Summary

| Layer | Choice | Why |
|---|---|---|
| App shell | **Tauri 2 on macOS; pywebview + pystray on Windows** | Native shell per platform with the shared React UI. |
| Core logic | **Rust on macOS; Python on Windows** | Platform-specific filesystem, key, process and network work. |
| UI | **React 19 + TypeScript + Vite** | Small popover UI; fast iteration; widely known. |
| Styling | **Plain CSS with design tokens** | Pixel-level control for a native feel, system font, light/dark. No component library to fight. |
| Core tests | `cargo test` on macOS; `python -m pytest` on Windows | Config logic runs against temp dirs; gateway clients run against mock HTTP responses. |
| UI tests | **Vitest + Testing Library** | Renders each screen state against a fake backend. |
| Package manager | **pnpm** | Already used here. |

## Rust crates

| Crate | Use |
|---|---|
| `tauri` (`tray-icon`, `macos-private-api`) | Shell, tray icon, transparent popover window |
| `tauri-plugin-autostart` | Launch at login (LaunchAgent on macOS) |
| `tauri-plugin-opener` | Opens links in the default browser |
| `toml_edit` | Reads and edits the Oxen CLI's `auth_config.toml` without disturbing the rest of it |
| `reqwest` (rustls) | Gateway calls: `/v1/models`, `/v1/messages` |
| `sysinfo` | Is Claude running, and when did it start |
| `serde` / `serde_json` (`preserve_order`) | Reads and writes Claude's JSON without reordering or dropping keys |
| `uuid`, `thiserror` | IDs, errors |

## Layout

A Cargo workspace with two crates, plus the web UI.

```
core/                    hitch-core: all logic, no UI dependencies. Fast to build and test.
  src/
    claude/              Everything specific to Claude Desktop
      paths.rs           Where Claude keeps things, per OS; managed-config detection
      library.rs         Read/write configLibrary and deployment mode
      process.rs         Detect, quit and reopen Claude (plus a fake for tests)
    gateway.rs           Oxen gateway client (validate key, list models, test message)
    secrets.rs           The API key store: the Oxen CLI's auth file, behind a trait
    state.rs             Hitch's own small state file (never holds the key)
    hitch.rs             The service the UI calls: connect, pause, models, remove
  examples/inspect.rs    Prints what Hitch sees on this machine. Read-only.
src-tauri/               The desktop shell
  src/
    lib.rs               Startup, wiring, background status watcher
    commands.rs          Tauri commands the UI calls
    tray.rs              Menu-bar icon and popover placement
src/                     React UI (the popover)
  lib/api.ts             The only place the UI talks to Rust
  lib/fake.ts            A fake backend with the same behavior, for tests and browser preview
  views/                 One file per screen
design/                  SVG sources for the app icon and menu-bar glyph
```

## Rules

- **The UI never touches the filesystem.** It calls `lib/api.ts`, which calls Rust commands.
- **Logic lives in `core/`, not in the shell.** `src-tauri` only wires commands to it.
- **Claude-specific code stays in `core/src/claude/`**, so a second agent is a sibling module, not a rewrite.
- **Core functions take a root directory**, never a hard-coded home path, so tests use temp dirs.
  `HITCH_CLAUDE_DIR` overrides the Claude data directory for manual testing without touching real config.

## Commands

```sh
pnpm install
pnpm tauri dev            # run the app against your real Claude Desktop
pnpm test                 # UI and script tests
pnpm test:rust            # core tests
pnpm check                # everything CI runs: tests, types, formatting, lint
pnpm tauri build          # produce the .app / .dmg
pnpm version check        # the version, and that every file agrees on it
pnpm version bump patch   # raise it everywhere (CI normally does this)
pnpm dev                  # UI only, in a browser, on a fake backend (http://localhost:1437)
cargo run -p hitch-core --example inspect   # what Hitch sees on this machine (read-only)
```

### Working without touching real Claude config

```sh
HITCH_CLAUDE_DIR=/tmp/hitch-sandbox/Claude-3p HITCH_STAY_OPEN=1 pnpm tauri dev
```

- `HITCH_CLAUDE_DIR` — sandbox mode. Config is written there, Hitch's state and its own copy of
  an Oxen auth file sit beside it (your real `~/.config/oxen` is not touched), and
  "Restart Claude" is simulated.
- `HITCH_STAY_OPEN` — debug builds only. The popover does not close when it loses focus.
- `HITCH_BASE_URL` — the gateway address offered before one is saved.

In a browser, `?state=connected` (or `restart`, `closed`, `paused`, `drifted`, `managed`, `models`,
`noclaude`, `badkey`, `offline`) picks the starting state. See `src/lib/preview.ts`.

The dev server uses port **1437**, not Tauri's default 1420, so it does not collide with other
Tauri projects on the same machine.

## CI and releases

Three GitHub Actions workflows in `.github/workflows/`:

| Workflow | When | What |
|---|---|---|
| `ci.yml` | Every push to `main`, every pull request, and before every release | On Linux: version consistency, UI tests, type-check and UI build, `rustfmt`, `clippy` and tests for the core. On macOS: `clippy` for the whole workspace, since the shell only builds there. |
| `release.yml` | A `vX.Y.Z` tag is pushed, or run by hand for a tag | Runs `ci.yml` at that tag, checks the tag matches the version in the code, builds a universal (Apple Silicon + Intel) `.dmg`, and publishes a GitHub release with it. |
| `bump-version.yml` | Run by hand | Raises the version in every file, commits to `main`, tags, and starts `release.yml`. |

The version lives in `package.json`, `src-tauri/tauri.conf.json`, both `Cargo.toml` files and
`Cargo.lock`. `scripts/version.mjs` reads, checks and bumps them together; CI fails if they differ.

Most checks run on Linux because the repository is private and macOS runner minutes are billed at
ten times the Linux rate. Only what needs a Mac runs on one.

**Signing.** Releases are signed with a Developer ID Application certificate and notarized with an
App Store Connect API key. Six repository secrets drive it: `APPLE_CERTIFICATE` (base64 `.p12`),
`APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_API_KEY` (the key's ID),
`APPLE_API_ISSUER` and `APPLE_API_KEY_P8` (the key file's contents). With no certificate secret
the build falls back to an ad-hoc signature (`signingIdentity: "-"`), which runs but makes macOS
refuse the first open. The certificate expires on 2027-12-20 and must be replaced before then.

A signed app runs under Apple's hardened runtime. `src-tauri/Entitlements.plist` allows the one
thing Hitch needs beyond the defaults, sending Claude a quit request, and `src-tauri/Info.plist`
holds the sentence macOS shows when it first asks the user to allow that.
