# Tech Stack

_Last updated: 2026-11-19_

## Summary

| Layer | Choice | Why |
|---|---|---|
| App shell | **Tauri 2** | One codebase for macOS, Windows and Linux; ~10 MB app; native tray, autostart and windowing. Electron is ~150 MB for a menu-bar utility. Native Swift would be macOS-only and mean a rewrite per OS. |
| Core logic | **Rust** | All file, keychain, process and network work. Matches Oxen's Rust codebase. Easy to unit test against a temp directory. |
| UI | **React 19 + TypeScript + Vite** | Small popover UI; fast iteration; widely known. |
| Styling | **Plain CSS with design tokens** | Pixel-level control for a native feel, system font, light/dark. No component library to fight. |
| Rust tests | `cargo test`, `tempfile`, `wiremock` | Config logic runs against temp dirs; the gateway client runs against a mock HTTP server. |
| UI tests | **Vitest + Testing Library** | Renders each screen state against a fake backend. |
| Package manager | **pnpm** | Already used here. |

## Rust crates

| Crate | Use |
|---|---|
| `tauri` (`tray-icon`, `macos-private-api`) | Shell, tray icon, transparent popover window |
| `tauri-plugin-positioner` | Places the popover under the menu-bar icon |
| `tauri-plugin-autostart` | Launch at login (LaunchAgent on macOS) |
| `tauri-plugin-opener` | Opens links in the default browser |
| `keyring` | Stores the Oxen API key in the OS keychain |
| `reqwest` (rustls) | Gateway calls: `/v1/models`, `/v1/messages` |
| `sysinfo` | Is Claude running, and when did it start |
| `serde` / `serde_json` (`preserve_order`) | Reads and writes Claude's JSON without reordering or dropping keys |
| `uuid`, `thiserror`, `tokio` | IDs, errors, async |

## Layout

```
src/                     React UI (popover)
  lib/api.ts             The only place the UI talks to Rust
  views/                 One file per screen
src-tauri/
  src/
    claude/              Everything specific to Claude Desktop
      paths.rs           Where Claude keeps things, per OS
      library.rs         Read/write configLibrary, deployment mode
      process.rs         Detect, quit and relaunch Claude
    gateway.rs           Oxen gateway client (validate key, list models)
    secrets.rs           Keychain access behind a trait
    state.rs             Hitch's own small state file
    commands.rs          Tauri commands the UI calls
    tray.rs              Menu-bar icon and popover window
```

## Rules

- **The UI never touches the filesystem.** It calls `lib/api.ts`, which calls Rust commands.
- **Claude-specific code stays in `claude/`**, so a second agent is a sibling module, not a rewrite.
- **Core functions take a root directory**, never a hard-coded home path, so tests use temp dirs.
  `HITCH_CLAUDE_DIR` overrides the Claude data directory for manual testing without touching real config.

## Commands

```sh
pnpm install
pnpm tauri dev          # run the app
pnpm test               # UI tests
cargo test --manifest-path src-tauri/Cargo.toml   # Rust tests
pnpm tauri build        # produce the .app / .dmg
```
