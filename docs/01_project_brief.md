# Oxen Hitch — Project Brief

_Last updated: 2026-11-19_

## What we are building

**Hitch** is a small menu-bar app that lets a non-technical person run **Claude Desktop** on
inference served by the **Oxen.ai gateway**. They install Hitch, paste their Oxen API key, click
Connect, and Claude Desktop (chat, Cowork, Code — the full harness) talks to Oxen instead of
Anthropic. No developer mode, no JSON editing, no hunting for files.

Claude Desktop is the first agent Hitch supports. The name says what it does: it hitches your agent
to Oxen. More agents can follow, so anything Claude-specific lives behind its own module.

## Why

Doing this by hand is possible but hostile to normal users. It took us:

1. Putting Claude Desktop into Developer Mode (Help → Troubleshooting → Enable Developer Mode).
2. Using Developer → Configure Third-Party Inference…, or hand-editing a JSON file with a random
   UUID name inside `~/Library/Application Support/Claude-3p/configLibrary/`.
3. Knowing the exact keys, including `"claudeAiImport": { "enabled": true }`.
4. Quitting and reopening Claude, because config is only read at launch.

Hitch reduces that to: paste key → Connect → Restart Claude.

## Who it is for

Someone who uses Claude Desktop, has an Oxen.ai account, and has never opened a terminal. Every
design decision is judged against that person. The bar for the UI is high: minimal, beautiful,
obvious. It should feel like a first-party Mac utility.

## What v1 does

| Capability | Behavior |
|---|---|
| Menu-bar presence | An icon in the macOS menu bar (like Docker / Dropbox). Click for a popover. No Dock icon. |
| Setup | Hitch uses the Oxen API key already saved by the Oxen CLI, or the user pastes one. It validates the key against the gateway, then writes Claude's config. |
| Connection status | Live state: connected / key rejected / gateway unreachable / Claude not installed. Menu-bar icon reflects it. |
| Restart Claude | Config applies at launch, so Hitch offers a "Restart Claude" button. It never restarts Claude without a click. |
| Switch back to Anthropic | One toggle returns Claude Desktop to normal Claude.ai sign-in, and back again, without losing the key. |
| Models | Every chat model on Oxen is added to Claude Desktop, including non-Claude ones, which Claude would otherwise hide. This happens when Hitch starts and whenever its window opens; a Refresh button does it on demand. The user can narrow the list and pick the default. |
| Launch at login | Optional. Hitch starts with the Mac and re-checks that Claude is still pointed at Oxen. |
| Host | Point Hitch at a different Oxen host. Typing just `hub.oxen.ai` or `localhost:3001` is enough; Hitch adds the scheme and the `/api/ai` path. A full URL also works. Defaults to `hub.oxen.ai`. |
| Remove | Take Hitch's profile out of Claude and restore what was applied before. The saved key is left alone. |

Not in v1: "Sign in with Oxen" (browser login instead of pasting a key), Windows/Linux builds,
agents other than Claude Desktop, auto-update, signed/notarized distribution.

## How Claude Desktop third-party mode actually works

This is the knowledge the whole app rests on. Sources: Anthropic's "Claude Desktop on 3P" docs
(`claude.com/docs/third-party/claude-desktop/…`: `configuration`, `configuration-format`,
`in-app-configuration`, `gateway`, `data-storage`, `mdm`) and reading the logic in the installed app
(Claude Desktop 6.6395.1). Items marked _(observed)_ come from the app, not the docs.

### Where config lives

Claude Desktop keeps a second data directory just for third-party mode:

| OS | Directory |
|---|---|
| macOS | `~/Library/Application Support/Claude-3p/` |
| Windows | `%LOCALAPPDATA%\Claude-3p\` (older builds: `%APPDATA%\Claude-3p\`; MSIX installs differ) |
| Linux | `~/.config/Claude-3p/` |

Inside it:

```
Claude-3p/
  claude_desktop_config.json        # holds "deploymentMode": "3p" | "1p"  (the user's mode choice)
  configLibrary/
    _meta.json                      # which saved configuration is applied
    <uuid>.json                     # one file per saved configuration
```

`_meta.json` _(observed)_:

```json
{ "appliedId": "<uuid>", "entries": [ { "id": "<uuid>", "name": "Default" } ] }
```

**The profile file name is not consistent across machines** — it is a random UUID. Never search
for it; read `appliedId` from `_meta.json`. IDs must match `^[a-f0-9-]{36}$` _(observed)_.

### The profile Hitch writes

```json
{
  "claudeAiImport": { "enabled": true },
  "inferenceProvider": "gateway",
  "inferenceGatewayBaseUrl": "https://hub.oxen.ai/api/ai",
  "inferenceGatewayApiKey": "<key>",
  "inferenceCredentialKind": "static",
  "inferenceModels": ["<default model id>", "<another>"]
}
```

- `inferenceProvider` — setting it is what activates third-party mode.
- `inferenceGatewayApiKey` — **Hitch writes it in plain text; Claude encrypts it on its next
  launch** and rewrites it as `__sealed__:v1:<base64>` (macOS Keychain item "Claude Safe Storage").
  This is documented behavior: "A tool that writes an unencrypted key still works, and the app
  encrypts that key at the next launch." A sealed value cannot be read back or moved to another
  machine, so Hitch needs the key from somewhere else: the Oxen CLI's settings file (see below).
- `inferenceGatewayAuthScheme` — `bearer` (default) or `x-api-key`. Oxen uses bearer, so we omit it.
- `inferenceModels` — optional. Overrides auto-discovery; the first entry is the default. Needed
  for non-Claude models because auto-discovery "shows only models whose IDs are recognizably Claude".
- `claudeAiImport.enabled` — lets the user import their Claude.ai chats into third-party mode.

### How the app decides between Anthropic (1P) and third-party (3P) mode _(observed)_

1. If no third-party config is applied → normal Anthropic mode.
2. Else if `Claude-3p/claude_desktop_config.json` has `"deploymentMode": "1p"` → Anthropic mode
   (this is what gets saved when the user picks "sign in to Claude.ai" on the login screen).
3. Else → third-party mode.

So **"Switch back to Anthropic" is just writing `"deploymentMode": "1p"`**, and switching on again
is writing `"3p"`. The profile and key stay put. This mirrors what Claude does itself.

### Rules that shape the product

- **Config is read at launch only.** Every change needs Claude to be fully quit and reopened.
- **Managed config wins.** If `/Library/Managed Preferences/com.anthropic.claudefordesktop.plist`
  (MDM) sets inference keys, local `configLibrary/` values are ignored. Hitch detects this and says
  so rather than appearing to work.
- **Developer Mode is not required** when writing `configLibrary/` directly. Hitch never touches it.
- A plain `defaults write com.anthropic.claudefordesktop …` in the user domain is not a documented
  config source. Don't use it.

### What the gateway must provide

- `POST {baseUrl}/v1/messages` with streaming and tool use — required.
- `GET {baseUrl}/v1/models` — optional for Claude, but Hitch uses it to validate the key and fill
  the model picker.
- Auth: `Authorization: Bearer <key>`.
- The base URL has **no** trailing `/v1`; Claude appends it. For Oxen: `https://hub.oxen.ai/api/ai`.

### Where the API key lives

Hitch keeps the key where the Oxen CLI already does: `~/.config/oxen/auth_config.toml`.

```toml
default_host = "hub.oxen.ai"

[[host_configs]]
host = "hub.oxen.ai"
auth_token = "..."

[[host_configs]]
host = "localhost:3001"
auth_token = "..."
```

Keys are per host, which matches Hitch's Host setting. If the file already has a token for the
chosen host, Hitch offers it and nothing needs pasting. A key pasted into Hitch is saved there for
that host, so the CLI can use it too. Hitch edits only that host's entry and never deletes a key.
It does not use the OS keychain.

## How Hitch behaves

- **It owns one profile.** Hitch adds its own entry named "Oxen" to `configLibrary/` and applies
  it. It never edits or deletes profiles it did not create, and it remembers what was applied
  before so Remove can put it back.
- **It never surprises the user.** No restart of Claude without a click. If Claude's config is
  changed behind Hitch's back, Hitch reports it and offers Reconnect instead of silently rewriting.
- **Writes are safe.** Atomic (temp file + rename), `0600` files, `0700` directories, unknown keys
  in existing files preserved.

## Open items

- `https://hub.oxen.ai/api/ai/v1/models` returned 404 on 2026-11-19 and the local dev server at
  `localhost:3001` was failing to compile, so the gateway client is tested against a mock only.
  It needs one real end-to-end run once either is up.
- Distribution: Developer ID signing, notarization, DMG, auto-update.
- "Sign in with Oxen" needs an OAuth or device-login endpoint on the hub.

## Where to look next

| File | What |
|---|---|
| `docs/02_tech_stack.md` | The stack and why |
| `docs/03_decisions.md` | Dated decisions and rationale |
| `AGENTS.md` | Development loop and conventions |
