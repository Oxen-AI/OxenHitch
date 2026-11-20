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
into `design/oxen-x-black.svg` and `design/oxen-x-white.svg`. `design/icon.svg` and `design/tray.svg` are generated from them, and `design/render-icons.sh`
renders the PNG/ICNS files in `src-tauri/icons/` from those. This replaced an invented placeholder glyph. Requested by Greg.

## 2026-11-19 — Restart means quit, wait, open, wait

"Restart Claude" asks Claude to quit the normal way (so it can save its state), waits until the
process is really gone, opens it, then waits until it is really running before reporting success.
Opening too early only brings the old process forward with its old settings; returning too early
leaves the UI saying Claude is not running. If Claude will not quit (it may be showing a dialog)
Hitch says so and does not open a second copy. The home screen also shows a "Claude Desktop" row
(Running / Not running) so it is clear what Hitch detected. Requested by Greg.

## 2026-11-19 — Read the keychain once per launch; no keychain in sandbox mode

Every keychain read can raise a macOS permission prompt, and unsigned dev builds get a new identity
on each rebuild, so "Always Allow" never sticks during development. Hitch was reading the key each
time the popover opened. Now the key is read once and held in memory for the life of the process
(`Remembered`), and a refused read is not cached so the next attempt can ask again. Sandbox mode
stores its throwaway key in an owner-only file instead (`FileSecrets`). A signed release build has
a stable identity, so users should see at most one prompt, if any. Reported by Greg.

## 2026-11-19 — The menu-bar icon is always full strength

The "not connected" icon used to be the mark at 42% opacity. Next to other menu-bar icons it looked
broken rather than informative. The mark is now solid and larger (18pt tall) in every state, and a
slash through it means Oxen is set up but switched off or no longer applied. Requested by Greg.

## 2026-11-19 — Development builds keep the key in a file, not the keychain

Reading the keychain once per launch was not enough during development: every code change rebuilds
the unsigned app, macOS treats it as a different program, and the permission prompt returns. Debug
builds now store the key in an owner-only file (`dev-key` in Hitch's app-data folder). Release
builds still use the keychain, where a signed app reads its own item without a prompt. Reported by
Greg.

## 2026-11-19 — Hitch lists Oxen's models in Claude itself, named `claude-via-{UUID}`

Claude Desktop only shows a model whose name reads as one of Anthropic's, so its own discovery
hides most of what Oxen offers. Hitch therefore writes `inferenceModels` itself:

- It reads Oxen's full list from `GET {host}/api/ai/models` and keeps models that output text and
  can run inference.
- Anthropic's models keep their names. Every other model is written as
  `{ "name": "claude-via-{UUID}", "labelOverride": "<display name>", "behavesAs": "claude-sonnet-5" }`,
  using the model's `uuid` field from that list.
- **The server side of this is not built yet.** Oxen's list has no `uuid` field and the gateway
  resolves `claude-via-<name>` by name only. Until a model has a `uuid`, Hitch writes the model's
  name reversed (`claude-via-inim-5-tpg`), which the gateway already resolves, and switches to the
  UUID by itself once the field appears. Greg is adding the server support.
- With nothing chosen in Hitch, every model is listed (Anthropic's first). Choosing models narrows
  the list; the first choice is Claude's default.
- Hitch remembers the last list it saw so it can rewrite the profile without the network.
- The list is kept up to date automatically (see "Sync Claude's model list automatically" below).

Decided by Greg.

## 2026-11-19 — Model subtitles cannot come from the config file

Claude's schema for an `inferenceModels` entry (read from the app, version 6.6395.1) is `name`,
`labelOverride`, `behavesAs`, `supports1m`, `prefer1m`, `anthropicFamilyTier`, `isFamilyDefault`,
`maxEffort`, `minDesktopVersion`. There is no description field. Claude does read a `description`
(cut to 100 characters) per model from the gateway's `GET /v1/models` response during discovery,
together with `display_name`, `behaves_as`, `max_effort_level`, `supports_1m` and
`anthropic_family_tier`. But discovery is skipped when `inferenceModels` is set, unless
`modelDiscoveryEnabled` is set explicitly. So subtitles need the gateway to describe its models,
and how discovered models combine with a configured list is untested.

## 2026-11-19 — The key lives in the Oxen CLI's settings file, not the keychain

This replaces three earlier decisions above: keeping the key in the OS keychain, reading the
keychain once per launch, and using a private file for development builds.

The keychain kept interrupting with permission prompts, and "Always Allow" does not hold for
unsigned builds. Meanwhile the Oxen CLI already stores a token per host in
`~/.config/oxen/auth_config.toml`, and those hosts line up with Hitch's Host setting. So:

- Hitch reads the token for the chosen host from that file. Anyone who has used the CLI has
  nothing to paste.
- A key pasted into Hitch is written to that file for that host, so the CLI gains it too.
- Changing host uses that host's saved key. If it has none, the current key is carried over.
- Hitch edits only the one host entry (with `toml_edit`, so the rest of the file is untouched),
  writes atomically, and keeps the file owner-only. A file it cannot parse is never overwritten.
- Remove no longer deletes the key: the CLI depends on it.

The trade-off is that the key sits in a plain-text file readable by the user's own programs,
rather than in the keychain. That is already true for every Oxen CLI user. Suggested by Greg.

## 2026-11-19 — Sync Claude's model list automatically

At first Hitch only rewrote the model list when the user changed something in Hitch. In practice
that meant Claude kept showing a stale list and nobody knew why. Now, every time Hitch checks the
gateway (at launch and whenever the popover opens) it compares what Oxen offers with what Claude's
profile holds, and rewrites the profile if they differ. The user is then asked to restart Claude,
as with any other change.

Guards, because each rewrite costs a restart:
- Only while Claude is set to use Oxen. Switched off, the list is refreshed on switching back on.
  A setup someone else changed is left alone.
- With every model shown, order is ignored, since Oxen may list models differently between calls.
- An empty answer never empties Claude's list: it means Oxen could not be asked.

A list written by hand into Hitch's profile is replaced. Reported by Greg.
