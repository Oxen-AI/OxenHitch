# Decisions

Append new decisions at the bottom with a date. Say what was decided and why.

## 2026-11-19 — Tauri 2 over Swift and Electron

Hitch must reach Windows and Linux after macOS. Tauri gives one codebase, a ~10 MB app and native
tray/autostart. Swift would be the most Mac-native but macOS-only. Electron is too heavy for a
menu-bar utility. Decided by Greg.

## 2026-11-19 — Write `configLibrary/` directly; do not use Developer Mode

Anthropic documents `Claude-3p/configLibrary/` as the local config source, and the in-app developer
UI just writes there. Writing it ourselves removes the two most painful manual steps (enabling
Developer Mode, using the config window). Developer Mode has no documented programmatic switch, so
depending on it would be fragile.

## 2026-11-19 — Find the profile through `_meta.json`, never by searching

The profile file name is a random UUID per machine. `_meta.json`'s `appliedId` names the active one.

## 2026-11-19 — Hitch owns its own profile entry

Hitch adds an entry named "Oxen" and applies it, rather than editing whatever profile is applied.
It never modifies or deletes profiles it did not create, and it records the previously applied ID so
Remove can restore it. Cost: a user who already hand-built an Oxen profile sees two entries in
Claude's config window. Worth it for a setup that is fully reversible.

## 2026-11-19 — Write the API key in plain text and let Claude seal it

Claude encrypts a plain-text key on its next launch (documented). Hitch cannot produce the
`__sealed__:v1:` format itself without Claude's keychain item, and should not try. The file is
`0600` and the plain-text window lasts until Claude next starts.

## 2026-11-19 — Hitch keeps its own copy of the key in the OS keychain

A sealed key cannot be read back. Hitch needs the key to test the connection and list models, and
to rewrite the profile after a change. So the key lives in the keychain under service
`ai.oxen.hitch`; it is never written to Hitch's own state file.

## 2026-11-19 — "Switch back to Anthropic" flips `deploymentMode`, nothing else

Claude treats `"deploymentMode": "1p"` in `Claude-3p/claude_desktop_config.json` as "the user chose
Anthropic sign-in" and ignores the third-party profile. Flipping that one value is instant,
loses nothing, and is exactly what Claude does itself. Deleting `configLibrary/` (the documented
full revert) is reserved for Remove.

## 2026-11-19 — Never restart Claude without a click

Config applies only at launch, but an automatic restart can interrupt a running chat or task.
Hitch shows "Restart Claude" and waits. Decided by Greg.

## 2026-11-19 — Paste-a-key for v1

"Sign in with Oxen" is the better experience but needs a login endpoint on the hub. The key field
is built so a sign-in button can become the primary path later. Decided by Greg.

## 2026-11-19 — Report config drift; do not silently repair it

If Claude's applied profile or mode changes behind Hitch's back, the user may have done it on
purpose. Hitch shows that Claude is no longer using Oxen and offers Reconnect.

## 2026-11-19 — Detect managed (MDM) config and say so

Managed preferences override local config. Without detection Hitch would look connected while
Claude ignores it.

## 2026-11-19 — Logic in a separate `hitch-core` crate

All file, network, keychain and process logic lives in `core/`, with no Tauri dependency. It
compiles and tests in seconds, and every function takes its directories as arguments so tests run
against temp folders. The Tauri crate is a thin shell.

## 2026-11-19 — Validate the key against the gateway before writing anything

Connect calls `GET {baseUrl}/v1/models` first. A mistyped key or wrong address is caught in Hitch,
with a plain-language message, instead of surfacing later as a confusing failure inside Claude.

## 2026-11-19 — The popover sizes itself and places itself

The window is exactly as tall as its content (the UI reports its height). We compute its position
from the tray icon's rectangle ourselves rather than using the positioner plugin. On first launch
macOS reports a placeholder icon position until the menu bar is laid out, so Hitch waits for a
position that is really inside a menu bar and falls back to the top-right corner.

## 2026-11-19 — Monochrome UI, color only for state

Black-and-white surfaces with the system font; green, amber and red are reserved for connection
state. It reads as native on macOS and lets the Oxen mark be the only brand element.

## 2026-11-19 — Sandbox mode for development

`HITCH_CLAUDE_DIR` redirects every write to a scratch directory and simulates restarting Claude, so
the app can be exercised end to end without changing a developer's real Claude setup.

## 2026-11-19 — Ask for a host, not a URL

The setting is a "Host" field. `hub.oxen.ai` becomes `https://hub.oxen.ai/api/ai`; a host on this
machine (`localhost`, `127.x`) uses http. A full URL is still accepted for unusual setups, and the
field shows the full URL only when the host alone would not reproduce it. A non-technical user
should never have to know the gateway path. Requested by Greg.

## 2026-11-19 — Use the Oxen "x" mark everywhere

The app icon, the in-app mark and the menu-bar glyph all come from Oxen's own logo files, copied
into `design/oxen-x-black.svg` and `design/oxen-x-white.svg`. `design/icon.svg`, `design/tray.svg`
and `design/tray-off.svg` are generated from them; the PNG/ICNS files in `src-tauri/icons/` are
rendered from those. This replaced an invented placeholder glyph. Requested by Greg.

## 2026-11-19 — Restart means quit, wait, open, wait

"Restart Claude" asks Claude to quit the normal way (so it can save its state), waits until the
process is really gone, opens it, then waits until it is really running before reporting success.
Opening too early only brings the old process forward with its old settings; returning too early
leaves the UI saying Claude is not running. If Claude will not quit (it may be showing a dialog)
Hitch says so and does not open a second copy. The home screen also shows a "Claude Desktop" row
(Running / Not running) so it is clear what Hitch detected. Requested by Greg.
