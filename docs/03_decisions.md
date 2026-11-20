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
- The server side lives in OxenHub (added 2026-11-19 on the `gs/anthropic-messages-gateway`
  branch): `GET /api/ai/models` returns a `uuid` per model, and the Anthropic-compatible API
  resolves `claude-via-<uuid>`. A base model's uuid is a version 5 uuid derived from its name, so
  it is the same on every Hub; a custom model's uuid is its database id.
- Against a Hub that has no `uuid` field yet, Hitch writes the model's name reversed
  (`claude-via-inim-5-tpg`), which the gateway also resolves, and switches to the uuid by itself
  once the field appears.
- With nothing chosen in Hitch, every model is listed (Anthropic's first). Choosing models narrows
  the list; the first choice is Claude's default.
- Hitch remembers the last list it saw so it can rewrite the profile without the network.
- The list is kept up to date automatically (see "Sync Claude's model list automatically" below).

Decided by Greg.

## 2026-11-19 — Model subtitles come from the gateway, so Hitch keeps discovery on

How Claude Desktop (6.6395.1) builds its model picker, read from the app:

- An `inferenceModels` entry may hold only `name`, `labelOverride`, `behavesAs`, `supports1m`,
  `prefer1m`, `anthropicFamilyTier`, `isFamilyDefault`, `maxEffort`, `minDesktopVersion`. There is
  no description field.
- When a list is configured, the picker shows exactly those entries. For each one Claude looks up
  the model **with the same ID** in the gateway's `GET /v1/models` response and takes its
  `description` (cut to 100 characters) as the subtitle. With no description it falls back to
  built-in text, which exists only for real Claude models.
- Claude skips that gateway call when a list is configured ("inferenceModels obviates discovery"),
  unless `modelDiscoveryEnabled` is set explicitly.

So Hitch writes `"modelDiscoveryEnabled": true` whenever it writes a model list. The other half is
in OxenHub (added 2026-11-19, PR #2077): `GET /api/ai/v1/models` lists each non-Anthropic model a
second time under `claude-via-<uuid>`, with the model's `summary` as its `description`. Models
without a summary (fine-tunes, mostly) get no subtitle. Anthropic's own models are listed once and
keep Claude's built-in text.

The Hub side is tested and was checked against a running server. Whether Claude Desktop then shows
the subtitles has not been seen yet; that rests on reading its code.

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

## 2026-11-19 — Models are added at startup, and can be refreshed by hand

The sync above first ran only because the popover's page happens to load when the app starts. It is
now an explicit step in startup, in Rust, so it does not depend on the window. The Models screen
also has a Refresh button that runs the same sync on demand and reports the outcome: "Up to date",
or "Updated. Restart Claude to see the changes." Requested by Greg.

## 2026-11-19 — Private GitHub repository, CI, and tag-driven releases

The code lives at `Oxen-AI/OxenHitch`, private for now; it can be made public once builds are
signed. Decided by Greg.

- Releases are driven by tags. A `vX.Y.Z` tag builds and publishes the installer, and the release
  workflow reruns every check first so a tag can never ship something untested.
- A "Bump version" workflow does the version edit, commit, tag and release in one click, because
  the version lives in five files and doing it by hand invites a mismatch. It starts the release
  workflow directly: a tag pushed by a workflow does not trigger other workflows.
- One universal `.dmg` rather than one per chip, so there is a single download.
- Builds are ad-hoc signed until Apple Developer ID secrets are added. The release notes say so
  and give the one command needed to open the app.
- The content-security policy now allows Tauri's own IPC channel (`ipc:`). Without that the app
  silently fell back to a slower message-passing path.

## 2026-11-19 — Host is one tap from the home screen

- The home screen has a **Host** row next to **API key**. Both open the Connection screen with
  that field focused. Before, the host could only be changed behind the row labelled "API key",
  which gave no hint that it was there.
- The line under the headline no longer repeats the host, since the row shows it.
- Changing the host in Connection says when a key is already saved for the new host, the same
  way first run does, so moving between hosts (for example hub.oxen.ai and a local Oxen) needs
  nothing pasted.

## 2026-11-19 — Claude's own icon, read from the installed app

- The home screen shows Claude's real app icon in place of the generic chat bubble. Hitch reads
  it at run time from the Claude app on the machine (`CFBundleIconFile` in its `Info.plist`,
  converted to a 128 px PNG with macOS's `sips`). So it always matches the app the user has,
  and no Anthropic artwork is stored in this repository or shipped inside Hitch.
- The chat bubble stays as the fallback when the icon cannot be read, and on Windows and Linux
  until icon reading exists there.

## 2026-11-19 — Signed and notarized releases

- Releases are signed with the Developer ID certificate of Curious Inspiration LLC (team
  `9K93LTJDV5`), the Apple developer account available today. The signature names that company,
  not Oxen. Moving to an Oxen account later means a new certificate and new secrets, nothing else.
- Notarization uses an App Store Connect API key rather than an Apple ID and app-specific
  password, so it is not tied to one person's login.
- Apple only lets the Account Holder create a Developer ID certificate, and not through the API.
  The private key and signing request were made locally and the request uploaded by hand.
- The app declares the Apple-events entitlement and a usage sentence, because restarting Claude
  sends it a quit request and a hardened-runtime app must be allowed to do that.

## 2026-11-19 — The README covers Claude Code and Developer Mode

- Hitch does not configure the Claude Code command-line tool. It keeps its own settings, and two
  environment variables are all it needs, so the README documents those instead. Tested with
  Claude Code 2.1.329 against a local Oxen: a Claude model and a non-Claude model both answered.
- Developer Mode stays unnecessary for Hitch. The README documents it as the by-hand alternative
  and as a way to inspect what Hitch wrote. Its screenshots come from Stanford University IT's
  public guide, cropped to Claude's own menus, with credit. Replace them with our own before the
  repository goes public.

## 2026-11-19 — The popover opens on every Space and over full-screen apps

- A user reported that clicking the menu-bar icon did nothing until Hitch was killed and
  restarted. The popover is an ordinary window, and macOS keeps a window on the Space it was
  first shown on. Clicked from another Space, or from a full-screen app, it opened back on its
  original Space where nobody could see it; the next click then hid it. Getting an API key in a
  full-screen browser is enough to hit this on first run.
- The window now joins all Spaces and may sit over full-screen apps (`CanJoinAllSpaces`,
  `FullScreenAuxiliary`). Tauri only exposes the first of those, so this is set on the
  `NSWindow` directly.
- This cause was found by reading the code, not reproduced. Treat it as fixed only once the
  reporter confirms.

## 2026-11-19 — Connection settings are edited in place

- Host and API key are rows in a group with a real text field on the right, as in macOS System
  Settings. A first version drew the values as bare right-aligned text, which did not look
  editable; the fields now have a visible well and line up down the group.
- The saved key shows as a filled field (`••••••••1234`). Entering the field swaps that for
  "Paste a new key", so the instruction appears exactly when it is useful.
- **Save** is a small button in the header that only appears while there is a change to save.
  Before, a large disabled button sat in the middle of the screen most of the time.
- A caption appears only when a change needs explaining (which key a new host will use).
- The test message has a real **Send** button, and its answer gets its own line in the group, so
  a long model name can no longer wrap the row and hovering no longer paints half the group.
- Remove is a row in its own group; its confirmation opens in place.
- The Remove confirmation said the key would be forgotten. It is not: the key stays in the Oxen
  settings file because the Oxen CLI uses it. The text now says so.

## 2026-11-19 — First run opens a real window, and opening Hitch always shows something

A first-time user saw nothing after opening Hitch, then found a Dock icon that did nothing
("No Available Windows"). Reproduced: on a first launch the welcome window never reached the screen.

- **Cause.** macOS started Hitch as a Dock app, and Hitch switched itself to menu-bar-only a
  moment later. That switch deactivated the app, and the popover hides when it loses focus, so
  the welcome window was dismissed as soon as it opened. The Dock icon was left behind in the
  Dock's recent-apps area.
- **`LSUIElement`** in `Info.plist` makes Hitch a menu-bar app from its first moment, so there is
  no switch and no Dock icon.
- **Before setup the window opens in the middle of the screen and stays put** when it loses
  focus. Setting up means leaving for the browser to copy a key; the window must still be there
  on the way back. Once set up it is the popover again and hides on blur.
- **Opening Hitch always shows its window**: a manual launch when already set up (only a launch
  at login stays quiet), and opening the app again while it is running (Applications, Spotlight,
  Dock). The second also rescues anyone whose menu-bar icon is hidden behind the notch.
- **Right after setup the home screen says where Hitch lives**, once, with a small picture of the
  menu bar. It goes away with "Got it".
- Checked on a sandboxed copy of the built app by asking macOS whether the window is on screen:
  it was not before these changes, and is now, both on first launch and after reopening.

## 2026-11-20 — Offer only the models Oxen's agent offers

Hitch listed every model on Oxen that writes text and can be run: 84 on the Hub it was tried
against. Claude's picker filled with small models, old snapshots and fine-tunes (`ox-...`), and
most of them fail in Claude on the first message, since Claude sends tools with every request.

A model from `GET /api/ai/models` is now kept only if all of these hold:

- `capabilities.output` includes `text`, `capabilities.tools` is `true`, and `can_run_inference`
  is not `false`. This is the Hub's own test for a model its agent can run
  (`Oxen.Agent.tool_capable?`, and `isAgentCapable` in the web app).
- `summary` is not empty. Oxen writes a summary for each model it curates. This also means every
  model in the picker has a line under its name.

On that Hub the list went from 84 models to 20. The rule is in `gateway::agent_model`.

A model someone chose in Hitch earlier and that no longer passes stays in their list under its
plain name, as a chosen model Oxen stopped listing always has.

## 2026-11-20 — Restart Claude keeps trying while Claude updates

The Restart button showed an error on a Mac where Claude had an update waiting. Claude's log
showed why: it installed the update as it quit (6.6395.1 to 6.28899.2) and was gone for about
20 seconds. Hitch asked macOS to open Claude the moment the old process was gone, once, and gave
up when that failed.

Hitch now asks again every 2 seconds for up to 60 seconds, and only then reports that Claude did
not open, with the reason macOS gave. Waiting for Claude to quit is still 20 seconds.

The error itself was not captured, and an update cannot be staged on demand, so this is tested
with a scripted Claude that refuses to open at first (`core/src/claude/process.rs`), and by
restarting the real Claude with `cargo run -p hitch-core --example restart`.
