# Tech Stack

_Last updated: 2026-11-19_

## Summary

| Layer | Choice | Why |
|---|---|---|
| App shell | **Tauri 2** | One codebase for macOS, Windows and Linux; ~10 MB app; native tray, autostart and windowing. Electron is ~150 MB for a menu-bar utility. Native Swift would be macOS-only and mean a rewrite per OS. |
| Core logic | **Rust** | All file, key, process and network work. Matches Oxen's Rust codebase. Easy to unit test against a temp directory. |
| UI | **React 19 + TypeScript + Vite** | Small popover UI; fast iteration; widely known. |
| Styling | **Plain CSS with design tokens** | Pixel-level control for a native feel, system font, light/dark. No component library to fight. |
| Rust tests | `cargo test`, `tempfile`, `wiremock` | Config logic runs against temp dirs; the gateway client runs against a mock HTTP server. |
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
pnpm test                 # UI tests
pnpm test:rust            # core tests
pnpm tauri build          # produce the .app / .dmg
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
