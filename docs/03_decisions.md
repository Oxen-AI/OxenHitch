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
